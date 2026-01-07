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
    cargo build -p api-grpc


alias c := check-all
# Run all checks (linting, tests, etc.)
check-all:
    #!/usr/bin/env bash
    {{initialise}} "check-all"
    scripts/check-all.sh


alias d := docs
# Creates and serves the documentation site (clean: 'c' to clean)
docs clean="":
    #!/usr/bin/env bash
    set -euxo pipefail
    {{initialise}} "docs"

    if [ "{{clean}}" = "c" ]; then
        rm -rf target/doc
        cargo clean
        rm -rf docs/src/code
    fi

    # Rebuild rustdoc with custom header (sidebar/back link)
    RUSTDOCFLAGS="--html-in-header doc-header.html" \
      cargo doc --no-deps --workspace

    # Sync rustdoc into the book source; --no-times ensures mdBook notices updates
    rsync -a --delete --no-times target/doc/ docs/src/code/

    # Optional: trigger mdBook's live-reload watcher
    touch docs/src/_reload.md || true

    # Serve the docs (blocks until stopped)
    cd docs && mdbook serve -o


alias eb := enter-backend
# Enter the backend container shell
enter-backend:
    #!/usr/bin/env bash
    {{initialise}} "enter-backend"
    docker exec -it vpr-dev /bin/sh


alias f := format
# Enter the backend container shell
format:
    #!/usr/bin/env bash
    {{initialise}} "format"
    cargo fmt


alias g := gui
# Start the GUI app
gui:
    #!/usr/bin/env bash
    {{initialise}} "gui"
    # Load API_KEY from .env file
    if [ -f .env ]; then
        export $(grep -v '^#' .env | xargs)
    fi
    grpcui -proto crates/api-shared/vpr.proto -plaintext -H "x-api-key: ${API_KEY}" localhost:50051


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
# Start the dev app (build: 'b' - build images)
start-dev build="":
    #!/usr/bin/env bash
    {{initialise}} "start-dev"
    just _start-docker-daemon

    if [ "{{build}}" = "b" ]; then \
        docker compose -f compose.dev.yml down
        docker compose -f compose.dev.yml up --build; \
    else \
        docker compose -f compose.dev.yml up; \
    fi


# Check if Docker daemon is running, start Docker Desktop if not (macOS)
_start-docker-daemon:
    #!/usr/bin/env bash
    echo "Checking Docker daemon status..."
    
    # Check if Docker daemon is responsive
    if docker info >/dev/null 2>&1; then
        echo "Docker daemon is running"
        exit 0
    fi
    
    echo "Docker daemon is not running"
    
    # Check if we're on macOS and Docker Desktop is available
    if [[ "$OSTYPE" == "darwin"* ]] && [[ -d "/Applications/Docker.app" ]]; then
        echo "Starting Docker Desktop..."
        open -a Docker
        
        # Wait for Docker daemon to start (with timeout)
        echo "Waiting for Docker daemon to start..."
        for i in {1..60}; do
            if docker info >/dev/null 2>&1; then
                echo "Docker daemon is now running (took ${i} seconds)"
                exit 0
            fi
            echo -n "."
            sleep 1
        done
        
        echo ""
        echo "Timeout: Docker daemon did not start within 60 seconds"
        echo "Please check Docker Desktop manually"
        exit 1
    else
        echo "Docker Desktop not found or not on macOS"
        echo "Please start Docker manually or install Docker Desktop"
        exit 1
    fi


alias sp := start-prod
# Start the dev app (build: 'b' will also build the images)
start-prod build="":
    #!/usr/bin/env bash
    {{initialise}} "start-prod"
    just _start-docker-daemon

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
