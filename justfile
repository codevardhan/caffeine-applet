name := 'caffeine-applet'
appid := 'com.github.codevardhan.caffeine-applet'

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))

bin-src := 'target' / 'release' / name
bin-dst := base-dir / 'bin' / name

desktop := appid + '.desktop'
desktop-src := 'resources' / desktop
desktop-dst := clean(rootdir / prefix) / 'share' / 'applications' / desktop

appdata := appid + '.metainfo.xml'
appdata-src := 'resources' / appdata
appdata-dst := clean(rootdir / prefix) / 'share' / 'appdata' / appdata

icons-src := 'resources' / 'icons' / 'hicolor'
icons-dst := clean(rootdir / prefix) / 'share' / 'icons' / 'hicolor'

app-icons-src-dir := icons-src / 'scalable' / 'apps'
app-icons-dst-dir := icons-dst / 'scalable' / 'apps'

# Default recipe which runs `just build-release`
default: build-release

# Runs `cargo clean`
clean:
    cargo clean

# Removes vendored dependencies
clean-vendor:
    rm -rf .cargo vendor vendor.tar

# `cargo clean` and removes vendored dependencies
clean-dist: clean clean-vendor

# Compiles with debug profile
build-debug *args:
    cargo build {{args}}

# Compiles with release profile
build-release *args: (build-debug '--release' args)

# Compiles release profile with vendored dependencies
build-vendored *args: vendor-extract (build-release '--frozen --offline' args)

# Runs a clippy check
check *args:
    cargo clippy --all-features {{args}} -- -W clippy::pedantic

# Runs a clippy check with JSON message format
check-json: (check '--message-format=json')

# The lint gate CI enforces. Scoped to the binary on purpose: the test module
# still carries warnings (unused_must_use, field_reassign_with_default) that are
# being cleaned up separately, and gating on them now would only teach everyone
# to ignore a red build.
check-strict:
    cargo clippy --all-features -- -W clippy::pedantic -D warnings

# Run the application for testing purposes
run *args:
    env RUST_BACKTRACE=full cargo run --release {{args}}

# Checks the release metadata is consistent. Safe at any point in the cycle, so
# CI runs this on every push; `check-release` adds the pre-tag check on top.
#
# Each check here corresponds to a mistake that has actually shipped: 0.1.2 was
# announced in the AppStream metadata but never tagged, and its four screenshot
# URLs were pinned to a tag that did not exist yet, so all of them 404'd in the
# store listing.
check-metadata:
    #!/usr/bin/env bash
    set -euo pipefail
    status=0
    fail() { echo "FAIL: $1" >&2; status=1; }

    cargo_version="$(sed -n '0,/^version = /s/^version = "\(.*\)"/\1/p' Cargo.toml)"
    meta=resources/app.metainfo.xml

    # The newest <release> must match Cargo.toml. Stores read "what's new" from
    # the first entry, so a stale one advertises the wrong changelog.
    meta_version="$(grep -oPm1 '(?<=<release version=")[^"]+' "$meta")"
    if [ "$cargo_version" != "$meta_version" ]; then
      fail "Cargo.toml is $cargo_version but the newest metainfo release is $meta_version"
    fi

    # A release entry needs a date; appstreamcli does not treat a missing one as
    # an error, but stores sort on it.
    if ! grep -qm1 "<release version=\"$cargo_version\" date=\"[0-9-]\+\"" "$meta"; then
      fail "the $cargo_version release entry has no date attribute"
    fi

    # Screenshot URLs must float. Pinning them to a tag cannot work: the URL
    # lives inside the commit being tagged, so it can only ever reference the
    # previous release.
    if grep -qE 'raw\.githubusercontent\.com/[^"]*/v[0-9]+\.[0-9]+\.[0-9]+/' "$meta"; then
      fail "metainfo has tag-pinned raw.githubusercontent URLs; point them at main"
      grep -nE 'raw\.githubusercontent\.com/[^"]*/v[0-9]+\.[0-9]+\.[0-9]+/' "$meta" >&2
    fi

    # Every referenced screenshot must exist in-tree, or it 404s once pushed.
    while read -r shot; do
      [ -f "screenshots/$shot" ] || fail "metainfo references screenshots/$shot, which is missing"
    done < <(grep -oP '(?<=/screenshots/)[A-Za-z0-9_.-]+\.png' "$meta" | sort -u)

    # The store crops to 16:9. Odd sizes get letterboxed or rejected.
    if command -v identify >/dev/null; then
      for shot in screenshots/*.png; do
        size="$(identify -format '%wx%h' "$shot")"
        [ "$size" = "1536x864" ] || fail "$shot is $size, expected 1536x864"
      done
    else
      echo "note: ImageMagick not found, skipping screenshot dimension check" >&2
    fi

    if command -v appstreamcli >/dev/null; then
      appstreamcli validate --no-net "$meta" >/dev/null 2>&1 || {
        fail "appstreamcli validation failed"
        appstreamcli validate --no-net "$meta" >&2 || true
      }
    else
      echo "note: appstreamcli not found, skipping AppStream validation" >&2
    fi

    [ $status -eq 0 ] && echo "Release metadata OK for $cargo_version"
    exit $status

# Everything in check-metadata, plus the check that only makes sense right
# before tagging. Kept separate because it is *expected* to fail mid-cycle: the
# current version's tag already exists and points at an older commit until the
# version is bumped, so running this on every push would wedge CI red.
check-release: check-metadata
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_version="$(sed -n '0,/^version = /s/^version = "\(.*\)"/\1/p' Cargo.toml)"

    # Re-tagging a published version silently rewrites history for anyone who
    # already fetched it, so make it a deliberate choice rather than a surprise.
    if git rev-parse -q --verify "refs/tags/v$cargo_version" >/dev/null; then
      if [ "$(git rev-parse "v$cargo_version^{commit}")" = "$(git rev-parse HEAD)" ]; then
        echo "note: v$cargo_version already points at HEAD" >&2
      else
        echo "FAIL: v$cargo_version already exists and points elsewhere; moving it rewrites a published tag" >&2
        exit 1
      fi
    fi

    echo "Ready to tag v$cargo_version"

# Installs files
install:
    #!/usr/bin/env bash
    set -euo pipefail

    # Binary
    install -Dm0755 {{bin-src}} {{bin-dst}}

    # Desktop & AppStream
    install -Dm0644 resources/app.desktop {{desktop-dst}}
    install -Dm0644 resources/app.metainfo.xml {{appdata-dst}}
	
    # Icons: copy and rename
    mkdir -p {{app-icons-dst-dir}}
    shopt -s nullglob
    for src in {{app-icons-src-dir}}/*.svg; do
      base="$(basename "$src")"
      case "$base" in
        # canonical default icon name
        icon.svg)
          install -Dm0644 "$src" "{{app-icons-dst-dir}}/{{appid}}.svg"
          ;;
        # any extra variants you name as icon-*.svg
        icon-*.svg)
          suffix="${base#icon}"                # e.g. -busy.svg
          install -Dm0644 "$src" "{{app-icons-dst-dir}}/{{appid}}${suffix}"
          ;;
        # your coffee assets from Inkscape
        # symbolic variant referenced by the desktop entry's Icon= key
        coffee-symbolic.svg)
          install -Dm0644 "$src" "{{app-icons-dst-dir}}/{{appid}}-symbolic.svg"
          ;;
        coffee-full.svg|active.svg)
          # panel "on" state only. The default {{appid}}.svg comes from icon.svg,
          # which is a full-colour launcher icon rather than this recoloured mark.
          install -Dm0644 "$src" "{{app-icons-dst-dir}}/{{appid}}.On.svg"
          ;;
        coffee-empty.svg|inactive.svg)
          install -Dm0644 "$src" "{{app-icons-dst-dir}}/{{appid}}.Off.svg"
          ;;
        # already namespaced files (rare)
        {{appid}}*.svg)
          install -Dm0644 "$src" "{{app-icons-dst-dir}}/$base"
          ;;
        # skip anything else to avoid stray filenames
        *)
          echo "Skipping unknown icon: $base" >&2
          continue
          ;;
      esac
    done

# Uninstalls installed files
uninstall:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -f {{bin-dst}} {{desktop-dst}} {{appdata-dst}}
    rm -f \
      {{app-icons-dst-dir}}/{{appid}}.svg \
      {{app-icons-dst-dir}}/{{appid}}.On.svg \
      {{app-icons-dst-dir}}/{{appid}}.Off.svg \
      {{app-icons-dst-dir}}/{{appid}}-*.svg


# Vendor dependencies locally
vendor:
    #!/usr/bin/env bash
    mkdir -p .cargo
    cargo vendor --sync Cargo.toml | head -n -1 > .cargo/config.toml
    echo 'directory = "vendor"' >> .cargo/config.toml
    echo >> .cargo/config.toml
    echo '[env]' >> .cargo/config.toml
    if [ -n "${SOURCE_DATE_EPOCH}" ]
    then
        source_date="$(date -d "@${SOURCE_DATE_EPOCH}" "+%Y-%m-%d")"
        echo "VERGEN_GIT_COMMIT_DATE = \"${source_date}\"" >> .cargo/config.toml
    fi
    if [ -n "${SOURCE_GIT_HASH}" ]
    then
        echo "VERGEN_GIT_SHA = \"${SOURCE_GIT_HASH}\"" >> .cargo/config.toml
    fi
    tar pcf vendor.tar .cargo vendor
    rm -rf .cargo vendor

# Extracts vendored dependencies
vendor-extract:
    rm -rf vendor
    tar pxf vendor.tar

