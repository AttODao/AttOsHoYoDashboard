{ inputs, pkgs, ... }:

let
  ui = [ "banner-design" "brand" "design" "design-system" "slides" "ui-styling" "ui-ux-pro-max" ];
  impeccable = pkgs.runCommand "impeccable-engine" {
    nativeBuildInputs = [ pkgs.autoPatchelfHook ];
    buildInputs = [ pkgs.stdenv.cc.cc.lib ];
  } ''
    install -Dm755 ${inputs.impeccable-cli}/bin/impeccable "$out/bin/impeccable"
    install -Dm644 ${inputs.impeccable-cli}/LICENSE "$out/share/licenses/impeccable/LICENSE"
  '';
in
{
  packages = [ impeccable pkgs.python3 ];
  # The upstream skill launcher uses this engine without downloading another copy.
  env.IMPECCABLE_BIN = "${impeccable}/bin/impeccable";

  files.".pi/settings.json".json.skills = [
    "${inputs.skill-rust}"
    "${inputs.skill-impeccable}/.pi/skills/impeccable"
  ] ++ map (name: "${inputs.skill-ui}/.claude/skills/${name}") ui;
}
