{ inputs, pkgs, ... }:

let
  ui = [ "banner-design" "brand" "design" "design-system" "slides" "ui-styling" "ui-ux-pro-max" ];
  skills = pkgs.runCommand "attos-project-skills" {
    nativeBuildInputs = [ pkgs.autoPatchelfHook ];
    buildInputs = [ pkgs.stdenv.cc.cc.lib ];
  } ''
    destination="$out/share/attos-project-skills"
    mkdir -p "$destination/LICENSES"
    cp -RL ${inputs.skill-rust} "$destination/rust-skills"
    for name in ${builtins.concatStringsSep " " ui}; do
      cp -RL ${inputs.skill-ui}/.claude/skills/"$name" "$destination/$name"
    done
    cp -RL ${inputs.skill-impeccable}/.pi/skills/impeccable "$destination/impeccable"
    chmod -R u+w "$destination"
    install -m755 ${inputs.impeccable-cli}/bin/impeccable "$destination/impeccable/scripts/impeccable"
    install -m644 ${inputs.impeccable-cli}/LICENSE "$destination/LICENSES/impeccable.txt"
    install -m644 ${inputs.skill-ui}/LICENSE "$destination/LICENSES/ui-skills.txt"
  '';
in
{
  packages = [ skills pkgs.python3 ];
  tasks."project:skills" = {
    after = [ "devenv:files:cleanup" ];
    before = [ "devenv:enterShell" ];
    exec = ''
      set -e
      for name in rust-skills ${builtins.concatStringsSep " " ui} impeccable; do
        agent=agents
        if [ "$name" = impeccable ]; then agent=pi; fi
        directory="$DEVENV_ROOT/.$agent/skills"
        mkdir -p "$directory"
        target="$directory/$name"
        if [ -e "$target" ] && [ ! -L "$target" ]; then
          echo "Refusing to overwrite unmanaged skill directory: $target" >&2
          exit 1
        fi
        # -T replaces a directory link itself, never writes through it.
        ln -sfnT ${skills}/share/attos-project-skills/"$name" "$target"
      done
    '';
  };
}
