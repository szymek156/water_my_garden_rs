echo "Clippy"
cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features -- -D warnings

echo "Format"
cargo fmt --all

# echo "Audit"
# cargo audit

