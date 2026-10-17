# beeline dev tasks. Run `just` to list them.

version := `grep -m1 '^version' Cargo.toml | cut -d'"' -f2`

default:
    @just --list

# Build everything.
build:
    cargo build

# Launch the app.
run:
    cargo run

# Run the test suite.
test:
    cargo test

# Format + lint gate.
check:
    cargo fmt --check
    cargo clippy -- -D warnings
    cargo test

# Build beeline.app (macOS) into target/release/bundle/osx/ and sign it
# ad-hoc. Without a bundle signature, some Macs call the app "damaged".
# This does not notarise it: downloads still get the Gatekeeper warning.
bundle-mac:
    rm -rf target/release/bundle
    cargo bundle --release --format osx
    codesign --force --sign - target/release/bundle/osx/beeline.app
    codesign --verify --strict target/release/bundle/osx/beeline.app

# Build beeline.app and copy it into /Applications, replacing any older copy.
install-mac: bundle-mac
    rm -rf /Applications/beeline.app
    cp -R target/release/bundle/osx/beeline.app /Applications/

# Build a drag-to-install disk image (beeline.app + an Applications link)
# at dist/beeline-<version>.dmg.
dmg: bundle-mac
    rm -rf target/release/bundle/dmg-stage
    mkdir -p target/release/bundle/dmg-stage dist
    cp -R target/release/bundle/osx/beeline.app target/release/bundle/dmg-stage/
    ln -s /Applications target/release/bundle/dmg-stage/Applications
    hdiutil create -quiet -ov -volname beeline -fs HFS+ -format UDZO \
        -srcfolder target/release/bundle/dmg-stage dist/beeline-{{version}}.dmg
    @echo "dist/beeline-{{version}}.dmg"

# Render the app icon PNGs, the README banner and the GitHub social
# preview from their SVG sources.
# The banner's wordmark needs the Futura font (it ships with macOS).
icons:
    resvg -w 1800 assets/logo/banner.svg assets/logo/banner.png
    resvg -w 1280 -h 640 assets/logo/social.svg assets/logo/social.png
    resvg -w 32 -h 32 assets/icon/icon.svg assets/icon/32x32.png
    resvg -w 64 -h 64 assets/icon/icon.svg assets/icon/32x32@2x.png
    resvg -w 128 -h 128 assets/icon/icon.svg assets/icon/128x128.png
    resvg -w 256 -h 256 assets/icon/icon.svg assets/icon/256x256.png
    resvg -w 512 -h 512 assets/icon/icon.svg assets/icon/512x512.png
    resvg -w 1024 -h 1024 assets/icon/icon.svg assets/icon/512x512@2x.png
