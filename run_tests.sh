#!/bin/bash

# Test runner script for redis-agent-worker
# This script helps run different types of tests with proper configuration

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}ℹ $1${NC}"
}

# Check if Docker is running
check_docker() {
    if ! docker ps >/dev/null 2>&1; then
        print_error "Docker is not running. Please start Docker first."
        exit 1
    fi
    print_success "Docker is running"
}

# Run all tests
run_all() {
    print_header "Running All Tests"
    check_docker
    cargo test --verbose
    print_success "All tests passed!"
}

# Run integration tests only
run_integration() {
    print_header "Running Integration Tests"
    check_docker
    cargo test --test integration_test --verbose
    print_success "Integration tests passed!"
}

# Run with detailed output
run_verbose() {
    print_header "Running Tests (Verbose)"
    check_docker
    RUST_LOG=debug cargo test -- --nocapture --test-threads=1
}

# Run specific test
run_specific() {
    if [ -z "$1" ]; then
        print_error "Please provide a test name"
        echo "Usage: $0 specific <test_name>"
        exit 1
    fi

    print_header "Running Test: $1"
    check_docker
    RUST_LOG=info cargo test "$1" -- --nocapture
}

# Run tests with coverage
run_coverage() {
    print_header "Running Tests with Coverage"
    check_docker

    if ! command -v cargo-tarpaulin &> /dev/null; then
        print_info "Installing cargo-tarpaulin..."
        cargo install cargo-tarpaulin
    fi

    cargo tarpaulin --verbose --all-features --workspace --timeout 300 --out Html --output-dir coverage
    print_success "Coverage report generated in coverage/index.html"
}

# Check code formatting and linting
run_checks() {
    print_header "Running Code Checks"

    print_info "Checking formatting..."
    cargo fmt -- --check
    print_success "Formatting OK"

    print_info "Running clippy..."
    cargo clippy --all-targets --all-features -- -D warnings
    print_success "Clippy OK"

    print_info "Building project..."
    cargo build --verbose
    print_success "Build OK"
}

# Quick test (integration tests only, no verbose output)
run_quick() {
    print_header "Running Quick Test Suite"
    check_docker
    cargo test --test integration_test
    print_success "Quick tests passed!"
}

# CI-like test run
run_ci() {
    print_header "Running CI Test Suite"
    check_docker

    run_checks

    print_info "Running tests..."
    RUST_LOG=info cargo test -- --nocapture

    print_success "All CI checks passed!"
}

# Watch mode (requires cargo-watch)
run_watch() {
    print_header "Running Tests in Watch Mode"
    check_docker

    if ! command -v cargo-watch &> /dev/null; then
        print_error "cargo-watch is not installed"
        print_info "Install with: cargo install cargo-watch"
        exit 1
    fi

    cargo watch -x test
}

# Show help
show_help() {
    cat << EOF
Redis Agent Worker - Test Runner

Usage: $0 [command]

Commands:
    all             Run all tests (default)
    integration     Run integration tests only
    verbose         Run tests with verbose output and debug logging
    specific <name> Run a specific test by name
    coverage        Run tests and generate coverage report
    checks          Run formatting and linting checks
    quick           Run quick test suite (integration tests, no verbose)
    ci              Run full CI test suite (checks + tests)
    watch           Run tests in watch mode (requires cargo-watch)
    help            Show this help message

Examples:
    $0 all
    $0 integration
    $0 specific test_queue_enqueue_dequeue
    $0 verbose
    $0 ci

Requirements:
    - Rust and Cargo installed
    - Docker running (for Redis testcontainers)
    - Git installed

Optional tools:
    - cargo-watch (for watch mode)
    - cargo-tarpaulin (for coverage)

EOF
}

# Main script logic
case "${1:-all}" in
    all)
        run_all
        ;;
    integration)
        run_integration
        ;;
    verbose)
        run_verbose
        ;;
    specific)
        run_specific "$2"
        ;;
    coverage)
        run_coverage
        ;;
    checks)
        run_checks
        ;;
    quick)
        run_quick
        ;;
    ci)
        run_ci
        ;;
    watch)
        run_watch
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        print_error "Unknown command: $1"
        echo ""
        show_help
        exit 1
        ;;
esac
