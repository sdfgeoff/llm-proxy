# LLM Proxy

A proxy for OpenAI-compatible API endpoints that records and visualizes usage metrics, performance data, and request/response archives.

## What it does

LLM Proxy sits between your applications and your LLM providers. Every request that passes through it is logged and tracked, giving you visibility into how models are being used across your team and projects.

It captures request and response payloads, token counts, timing data, and other performance metrics — and presents everything through a built-in web dashboard.

## Key capabilities

- **Usage monitoring** — track token counts, request volumes, and error rates across models and API keys
- **Performance tracking** — monitor latency, time to first token, and tokens per second
- **Request archives** — store full request and response payloads for debugging and auditing
- **Model routing** — map logical model names to different providers or routes
- **Web dashboard** — view charts, request logs, and payload archives without leaving your browser

## What it doesn't do

This project is not a billing system, does not enforce quotas or rate limits, and does not estimate costs. It's a monitoring and observability tool.
