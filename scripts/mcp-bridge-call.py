#!/usr/bin/env python3
"""Raw client for tauri-plugin-mcp-bridge's WebSocket control server.

Usage: mcp-bridge-call.py <port> <command> [json-args] [timeout-seconds]

Stands in for the @hypothesi/tauri-mcp-server MCP tools when they are not bound:
MCP servers bind at session start, so a session predating an .mcp.json change has
no `tauri` tool and no other route to the bridge.

The bridge listens on loopback only, so an app on another host must be reached
through a forwarded port. Protocol per
tauri-plugin-mcp-bridge-0.11.2/src/websocket.rs:564.
"""

import asyncio
import json
import sys

import websockets


async def call(port: int, command: str, args=None, timeout: float = 15.0):
    uri = f"ws://127.0.0.1:{port}"
    async with websockets.connect(uri, open_timeout=timeout) as ws:
        req = {"id": "1", "command": command}
        if args is not None:
            req["args"] = args
        await ws.send(json.dumps(req))
        deadline = asyncio.get_event_loop().time() + timeout
        while True:
            remaining = deadline - asyncio.get_event_loop().time()
            if remaining <= 0:
                raise TimeoutError(f"no response with id=1 for {command}")
            raw = await asyncio.wait_for(ws.recv(), timeout=remaining)
            msg = json.loads(raw)
            if msg.get("id") == "1":
                return msg


def main() -> int:
    port = int(sys.argv[1])
    command = sys.argv[2]
    args = json.loads(sys.argv[3]) if len(sys.argv) > 3 else None
    timeout = float(sys.argv[4]) if len(sys.argv) > 4 else 15.0
    try:
        print(json.dumps(asyncio.run(call(port, command, args, timeout))))
    except Exception as exc:  # noqa: BLE001 - CLI boundary
        print(json.dumps({"clientError": f"{type(exc).__name__}: {exc}"}))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
