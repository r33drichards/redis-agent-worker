{
  description = "Redis Agent Worker - A reliable Redis-based worker system for processing agent jobs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Define Rust toolchain
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" ];
        };

        # Common build inputs
        buildInputs = with pkgs; [
          openssl
          pkg-config
          git
        ] ++ lib.optionals stdenv.isDarwin [
          darwin.apple_sdk.frameworks.Security
          darwin.apple_sdk.frameworks.SystemConfiguration
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
          rustToolchain
        ];

        # Rust build environment
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

      in
      {
        # Package definition
        packages = {
          default = self.packages.${system}.redis-agent-worker;

          redis-agent-worker = rustPlatform.buildRustPackage {
            pname = "redis-agent-worker";
            version = "0.1.0";

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            inherit buildInputs nativeBuildInputs;

            # Skip tests during build (they require Docker/Redis)
            doCheck = false;

            meta = with pkgs.lib; {
              description = "A reliable Redis-based worker system for processing agent jobs";
              homepage = "https://github.com/yourusername/redis-agent-worker";
              license = licenses.mit;
              mainProgram = "redis-agent-worker";
            };
          };
        };

        # Development shell
        devShells.default = pkgs.mkShell {
          name = "redis-agent-worker-dev";

          buildInputs = buildInputs ++ (with pkgs; [
            # Rust toolchain
            rustToolchain
            cargo-watch
            cargo-edit
            cargo-audit

            # Development tools
            git
            docker
            docker-compose
            redis

            # Testing tools
            testcontainers

            # Build dependencies
            openssl
            pkg-config

            # Optional: useful utilities
            jq
            curl
            netcat
          ]);

          shellHook = ''
            echo "🦀 Redis Agent Worker Development Environment"
            echo ""
            echo "Rust version: $(rustc --version)"
            echo "Cargo version: $(cargo --version)"
            echo ""
            echo "Available commands:"
            echo "  make test          - Run all tests"
            echo "  make test-verbose  - Run tests with output"
            echo "  make build         - Build release binary"
            echo "  make redis-up      - Start Redis with Docker Compose"
            echo "  make redis-down    - Stop Redis"
            echo "  cargo run -- run   - Run the worker"
            echo ""
            echo "Or use the ./run_tests.sh script for advanced test options"
            echo ""

            # Set up environment variables
            export RUST_BACKTRACE=1
            export RUST_LOG=info

            # Add cargo bin to PATH
            export PATH="$PWD/target/debug:$PATH"
          '';

          # Environment variables
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };

        # CI/CD shell (minimal dependencies)
        devShells.ci = pkgs.mkShell {
          name = "redis-agent-worker-ci";

          buildInputs = buildInputs ++ (with pkgs; [
            rustToolchain
            docker
            git
          ]);

          shellHook = ''
            echo "CI Environment for Redis Agent Worker"
            export RUST_BACKTRACE=1
            export RUST_LOG=info
          '';
        };

        # Apps - for running the built package
        apps = {
          default = {
            type = "app";
            program = "${self.packages.${system}.redis-agent-worker}/bin/redis-agent-worker";
          };
        };

        # Formatter
        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
