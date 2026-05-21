When a file gets larger than 15kb or so, it is too large to work with easily, so you should refactor/split it.
Favor having tests in a separate file to the code under test.

Tested code is good code. Tests with lots of mocks are bad tests and indicate the code needs better architecture.

Before committing, run and resolve issues with:
cargo clippy
cargo test