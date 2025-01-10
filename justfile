default: release

app := "wluma"
version := `echo ${WLUMA_VERSION:-$(git describe --tags)}`
vendor-config := """
    [source.crates-io]
    replace-with = "vendored-sources"

    [source.vendored-sources]
    directory = "vendor"
"""

release: clean vendor
    mkdir -p dist

    git -c tar.tar.gz.command="gzip -cn" archive -o "dist/{{app}}-{{version}}.tar.gz" --format tar.gz --prefix "{{app}}-{{version}}/" "{{version}}"

    git -c tar.tar.gz.command="gzip -cn" archive -o "dist/{{app}}-{{version}}-vendored.tar.gz" --format tar.gz \
        `find vendor -type f -printf '--prefix={{app}}-{{version}}/%h/ --add-file=%p '` \
        --add-virtual-file '{{app}}-{{version}}/.cargo/config.toml:{{vendor-config}}' \
        --prefix "{{app}}-{{version}}/" "{{version}}"

    for file in dist/*; do \
        gpg --detach-sign --armor "$file"; \
    done

    rm -f "dist/{{app}}-{{version}}.tar.gz"

run *args:
    cargo run {{args}}

build *args:
    cargo build --locked {{args}}

lint:
    cargo fmt -- --check
    cargo clippy -- -Dwarnings

test:
    cargo test --locked

vendor:
     cargo vendor vendor

clean:
    rm -rf dist
    rm -rf vendor
