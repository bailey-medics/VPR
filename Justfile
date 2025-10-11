set shell := ["bash", "-c"]

default:
    just --list

initialise:= 'set -euxo pipefail
    initialise() {
        # Clear the terminal window title on exit
        echo -ne "\033]0; \007"
    }
    trap initialise EXIT
    just _terminal-description'


_terminal-description message=" ":
    echo -ne "\033]0;{{message}}\007"

alias aj := abbreviate-just
# Set up the description for terminal windows
abbreviate-just:
    #!/usr/bin/env bash
    {{initialise}} abbreviate-just
    alias_definition="alias j='just'"

    if grep -Fxq "$alias_definition" ~/.zshrc
    then
        echo "Alias already exists in ~/.zshrc"
    else
        echo "$alias_definition" >> ~/.zshrc
        echo "Alias added to ~/.zshrc"
    fi

    echo "Please run the following command to apply the changes to this terminal:"
    echo "source ~/.zshrc"


alias bl := build_local
# Build the local Rust binaries
build_local:
    #!/usr/bin/env bash
    {{initialise}} "build_local"
    cargo build -p api


alias d := docs
# Creates and serves the documentation site (clean: 'c' to clean)
docs clean="":
    #!/usr/bin/env bash
    set -euxo pipefail
    {{initialise}} "docs"

    if [ "{{clean}}" = "c" ]; then
        rm -rf target/doc
        cargo clean
        rm -rf handbook/src/code
    fi

    # Rebuild rustdoc with custom header (sidebar/back link)
    RUSTDOCFLAGS="--html-in-header doc-header.html" \
      cargo doc --no-deps --workspace

    # Sync rustdoc into the book source; --no-times ensures mdBook notices updates
    rsync -a --delete --no-times target/doc/ handbook/src/code/

    # Optional: trigger mdBook's live-reload watcher
    touch handbook/src/_reload.md || true

    # Serve the handbook (blocks until stopped)
    cd handbook && mdbook serve -o


alias eb := enter-backend
# Enter the backend container shell
enter-backend:
    #!/usr/bin/env bash
    {{initialise}} "enter-backend"
    docker exec -it quill_backend /bin/sh


alias g := gui
# Start the GUI app
gui:
    #!/usr/bin/env bash
    {{initialise}} "gui"
    grpcui -proto crates/api/proto/vpr/v1/vpr.proto -plaintext localhost:50051


alias pc := pre-commit
# Run pre-commit checks
pre-commit:
    #!/usr/bin/env bash
    {{initialise}} "pre-commit"
    pre-commit run --all-files


alias sdc := show-dev-containers
# Show the running dev containers
show-dev-containers:
    #!/usr/bin/env bash
    {{initialise}} "show-dev-containers"
    docker compose -f compose.dev.yml ps


alias sd := start-dev
# Start the dev app (build: 'b' will also build the images)
start-dev build="":
    #!/usr/bin/env bash
    {{initialise}} "start-dev"

    echo "Access the frontend at: http://$(ipconfig getifaddr en0)"

    # just dds

    if [ "{{build}}" = "b" ]; then \
        docker compose -f compose.dev.yml down
        docker volume rm -f [replace]frontend_node_modules >/dev/null 2>&1 || true
        cd frontend && yarn install && cd ..
        cd backend && poetry lock && poetry install && cd ..
        docker compose -f compose.dev.yml up --build; \
    else \
        docker compose -f compose.dev.yml up; \
    fi

alias sp := start-prod
# Start the dev app (build: 'b' will also build the images)
start-prod build="":
    #!/usr/bin/env bash
    {{initialise}} "start-prod"
    if [ "{{build}}" = "b" ]; then \
        docker compose -f compose.yml -f compose.prod.yml up --build; \
    else \
        docker compose -f compose.yml -f compose.prod.yml up; \
    fi

alias sc := stop
# Stop the containers
stop:
    #!/usr/bin/env bash
    {{initialise}} "stop"
    docker compose -f compose.yml -f compose.dev.yml down
    docker compose -f compose.yml -f compose.prod.yml down