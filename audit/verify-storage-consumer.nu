# Build a bounded validation workspace around the exact Evy source files.
# The original workspace currently declares an unavailable bbg/backend-unimem
# feature. Omit only that forwarding edge in temporary manifests; do not test
# or claim support for backend-unimem. Repository manifests remain untouched.
let source = ($env.FILE_PWD | path dirname)
let cyber = ($source | path dirname)
let target = (^mktemp -d /tmp/evy-fallible.XXXXXX | str trim)
let names = [evy_platform_caps evy_ecs_storage evy_engine_tasks evy_engine_dispatch evy_radio evy_dialect evy_diagnostic evy_engine_core]
let dependencies = {
  bbg: {path: ($cyber | path join "bbg/rs")}
  hemera: {package: "cyber-hemera", path: ($cyber | path join "hemera/rs")}
  nebu: {package: "strata-nebu", path: ($cyber | path join "strata/nebu/rs")}
  unimem: {path: ($cyber | path join "honeycrisp/unimem")}
  acpu: {path: ($cyber | path join "honeycrisp/acpu")}
  rane: {path: ($cyber | path join "honeycrisp/rane")}
}
{
  workspace: {
    resolver: "2"
    members: $names
    package: {version: "0.1.0", edition: "2024", license: "Cyber"}
    dependencies: $dependencies
  }
} | to toml | save ($target | path join "Cargo.toml")
for name in $names {
  let original = ($source | path join "crates" $name)
  let directory = ($target | path join $name)
  mkdir $directory
  let manifest = (open --raw ($original | path join "Cargo.toml")
    | str replace '"bbg/backend-unimem", ' '')
  let lib_path = ($original | path join "src/lib.rs")
  $"($manifest)\n[lib]\npath = \"($lib_path)\"\n" | save ($directory | path join "Cargo.toml")
  if $name == "evy_engine_core" {
    let example = ($original | path join "examples/hello.rs")
    $"\n[[example]]\nname = \"hello\"\npath = \"($example)\"\n" | save --append ($directory | path join "Cargo.toml")
  }
}
print $target
