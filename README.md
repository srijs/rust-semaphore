# Semaphore

Atomic counting semaphore that can help you control access to a common resource
by multiple processes in a concurrent system.

[![Build Status](https://travis-ci.org/srijs/rust-semaphore.svg?branch=master)](https://travis-ci.org/srijs/rust-semaphore)

## Features

- Effectively lock-free* semantics
- Provides RAII-style acquire/release API
- Implements `Send`, `Sync` and `Clone`

_* lock-free when not using the `shutdown` API_
