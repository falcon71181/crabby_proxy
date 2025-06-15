# 🦀 crabby\_proxy

**crabby\_proxy** is an experimental proxy server written in [Rust](https://www.rust-lang.org/).
It started out of boredom and curiosity — not fully sure what it's becoming yet, but it's turning into something interesting. 😅

---

## 🧠 Project Goals (Tentative)

* Learn systems-level network programming in Rust
* Build a fast and efficient TCP proxy (May be :<)
* Provide a Web-based control panel for runtime management
* Experiment with connection approval workflows
* Try out WebSocket and Redis integrations

---

## 🧱 Architecture Overview

```text
               +-----------------+
               |    Admin Web    |
               |    Interface    |
               | (HTTP + WS)     |
               +-------+---------+
                       | (control)
+---------------+------+------+---------------+
|               | Shared State |               |
|               | (AppState)   |               |
|               +------+-------+               |
|                      |                       |
|            +---------+---------+             |
|            |  Pending Requests |             |
|            |  (RwLock<Map>)    |             |
|            +---------+---------+             |
|                      |                       |
+------------+---------+---------+-------------+
             |                   |
     +-------+-------+   +-------+-------+
     | Proxy Listener|   |Active Conn Mgr|
     | (TCP Server)  |   | (Relay)       |
     +-------+-------+   +-------+-------+
             |                   |
     +-------v-------+   +-------v-------+
     | Client Request|   | Target Server |
     +---------------+   +---------------+
```

---

## 🛠️ Features (Current & Planned)

* [x] TCP Proxy with connection relay
* [x] SOCKS4/SOCKS5 support
* [x] HTTP/HTTPS support
* [ ] Shared state management with async locking
* [ ] Web interface (via HTTP + WebSockets) to control connections
* [ ] Redis-backed shared state for clustering / persistence
* [ ] API endpoints to approve/reject connections (by IP or rule)
* [ ] Logging, metrics, and healthcheck endpoints
* [ ] TLS passthrough / termination
* [ ] Pluggable filters (e.g., rate limits, geo-blocking)

---

## 🚧 TODOs & Ideas

* [ ] Replace in-memory shared state with Redis cache
* [ ] Secure admin interface with authentication
* [ ] Expose REST + WebSocket API for:

  * Connection approval/rejection
  * Real-time connection stats
  * Manual control of active sessions
* [ ] Add configuration via TOML + hot reload support
* [ ] Dockerfile + CI/CD pipeline
* [ ] Use `tokio-tracing` for observability

---

## 🤝 Contributions

PRs, issues, and weird ideas are always welcome.
This project is still in early WIP, so feel free to play around or suggest new directions.

---

## 📜 License

NOT YET

---
