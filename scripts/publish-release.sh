#!/usr/bin/env bash
# Attach every file of a dist directory to the Forgejo release of a tag and check the result.
#
# Usage: publish-release.sh <dist dir>
# Environment:
#   API            https://<forgejo>/api/v1/repos/<owner>/<repo>
#   FORGEJO_TOKEN  token allowed to create releases and upload assets
#   TAG            existing git tag, e.g. v1.8.0
#   PRERELEASE     true|false; a release created here gets this flag
#   RELEASE_BODY   description of a release created here (an existing release keeps its own)
#
# The release is created when missing. Assets with the same name are replaced, others are
# kept. Afterwards the published release is read back, every asset is downloaded through
# its public URL and checked with `sha256sum -c SHA256SUMS`.
set -euo pipefail

dist=${1:?usage: publish-release.sh <dist dir>}
: "${API:?}" "${FORGEJO_TOKEN:?}" "${TAG:?}" "${PRERELEASE:?}" "${RELEASE_BODY:?}"
case "$PRERELEASE" in true | false) ;; *) echo "PRERELEASE must be true or false" >&2; exit 1 ;; esac
test -s "$dist/SHA256SUMS" || { echo "$dist/SHA256SUMS is missing" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
auth=(-H "Authorization: token $FORGEJO_TOKEN")

mapfile -t files < <(find "$dist" -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort)
# SHA256SUMS must cover exactly the other files, so nothing is published unchecked.
listed=$(sed 's/^[0-9a-f]\{64\}  //' "$dist/SHA256SUMS" | LC_ALL=C sort)
present=$(printf '%s\n' "${files[@]}" | grep -vx SHA256SUMS)
if [ "$listed" != "$present" ]; then
  printf 'SHA256SUMS lists\n%s\nbut %s holds\n%s\n' "$listed" "$dist" "$present" >&2
  exit 1
fi

status=$(curl -sS -o "$work/release.json" -w '%{http_code}' "${auth[@]}" "$API/releases/tags/$TAG")
case "$status" in
  200) ;;
  404)
    jq -n --arg tag "$TAG" --arg body "$RELEASE_BODY" --argjson prerelease "$PRERELEASE" \
      '{tag_name: $tag, name: $tag, body: $body, draft: false, prerelease: $prerelease}' \
      > "$work/request.json"
    curl --fail-with-body -sS -X POST "${auth[@]}" -H 'Content-Type: application/json' \
      --data-binary @"$work/request.json" "$API/releases" > "$work/release.json"
    ;;
  *)
    cat "$work/release.json" >&2
    echo "Release lookup failed: HTTP $status" >&2
    exit 1
    ;;
esac
id=$(jq -er '.id' "$work/release.json")

curl --fail-with-body -sS "${auth[@]}" "$API/releases/$id/assets" > "$work/assets.json"
for name in "${files[@]}"; do
  for asset_id in $(jq -r --arg name "$name" '.[] | select(.name == $name) | .id' "$work/assets.json"); do
    curl --fail-with-body -sS -o /dev/null -X DELETE "${auth[@]}" "$API/releases/$id/assets/$asset_id"
  done
  curl --fail-with-body -sS -o /dev/null -X POST "${auth[@]}" \
    -F "attachment=@$dist/$name" "$API/releases/$id/assets?name=$name"
  echo "uploaded $name"
done

# Read back the release by tag and check every asset by name, size and checksum.
curl --fail-with-body -sS "${auth[@]}" "$API/releases/tags/$TAG" > "$work/published.json"
jq -e --argjson prerelease "$PRERELEASE" '.draft == false and .prerelease == $prerelease' \
  "$work/published.json" > /dev/null || {
  echo "Release $TAG is a draft or its prerelease flag is not $PRERELEASE" >&2
  exit 1
}
mkdir "$work/download"
for name in "${files[@]}"; do
  url=$(jq -er --arg name "$name" --argjson size "$(stat -c %s "$dist/$name")" \
    '[.assets[] | select(.name == $name and .size == $size)] | if length == 1 then .[0].browser_download_url else error("missing") end' \
    "$work/published.json") || {
    echo "Release $TAG has no single asset $name of the built size" >&2
    exit 1
  }
  curl --fail -sSL "${auth[@]}" -o "$work/download/$name" "$url"
done
(cd "$work/download" && sha256sum --strict -c SHA256SUMS)
echo "release $TAG: ${#files[@]} assets published and verified"
