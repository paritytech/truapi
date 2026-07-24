Output "rust/crates/truapi-host-cli/recordings/signing-host-diagnosis.gif"

Require bun
Require target/debug/truapi-host

Set Shell "bash"
Set FontSize 18
Set FontFamily "FiraCode Nerd Font"
Set Width 1200
Set Height 720
Set Padding 24
Set Margin 16
Set MarginFill "#11111b"
Set BorderRadius 10
Set WindowBar "ColorfulRight"
Set Theme "Catppuccin Mocha"
Set Framerate 24
Set PlaybackSpeed 6.0
Set TypingSpeed 8ms

Hide
Type "export PS1='❯ '"
Enter
Type "unset NO_COLOR"
Enter
Type "export FORCE_COLOR=1"
Enter
Type "DIAGNOSIS_BASE_PATH=${TRUAPI_VHS_SIGNING_BASE_PATH:-$(mktemp -d /tmp/truapi-vhs-signing.XXXXXX)}"
Enter
Type "clear"
Enter
Sleep 1s
Show

Type "target/debug/truapi-host signing-host --base-path $DIAGNOSIS_BASE_PATH --product-id truapi-playground.dot --script rust/crates/truapi-host-cli/js/scripts/battery.ts --auto-accept; echo; echo 'Signing-host diagnosis complete'"
Enter
Sleep 90s
