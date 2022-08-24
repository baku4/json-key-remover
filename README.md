# json-key-remover
Remove specific keys from `json`

## Usage
### Binary
```bash
# File to file
json-key-remover -i input.json -o output.json -k unnecessary_key
# Pipe to pipe
wget -q -O ${interface} | json-key-remover -k unnecessary_key | head
# Remove multiple keys
json-key-remover -i input.json -o output.json -k key_1,key_2,key_3
```
### `Rust` library
```rust
use json_key_remover::KeyRemover;

// Init
let buffer_size = 64*1024;
let keys_to_remove = vec!["key_1".to_string(), "key_2".to_string()];
let mut key_remover = KeyRemover::init(buffer_size, keys_to_remove);
// Run
key_remover.process(reader, writer);
```

## Build
### With `cargo`
```bash
cargo build --release
```