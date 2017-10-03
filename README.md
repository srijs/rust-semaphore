# Semaphore

_Atomic counting semaphore_

A datastructure that can help you control access to a common resource by multiple processes in a concurrent system.

## Features

- Provides RAII-style atomic acquire and release
- Implements `Send`, `Sync` and `Clone`
- Can block until count to drops to zero (useful for implementing shutdown)
