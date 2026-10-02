+++
title = "Architecture"
video = "https://media.example.invalid/engineer/architecture.mp4"
duration_minutes = 9
doc_url = "/architecture"
+++

- SaaS platform + **gateway agent** installed at the customer's.
- The agent opens an **outbound encrypted connection** (443/tcp): no inbound flow.
- Clock synchronization (NTP) is required for tokens.
