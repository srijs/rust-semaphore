# Semaphore

Atomic counting semaphore that can help you control access to a common resource
by multiple processes in a concurrent system.

## Features

- Fully lock-free* semantics
- Provides RAII-style acquire/release API
- Implements `Send`, `Sync` and `Clone`

_* when not using the `shutdown` API_