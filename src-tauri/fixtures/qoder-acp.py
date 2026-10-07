"""Offline ACP transport fixture. It never calls a model or reads authentication."""
import json
import sys


def emit(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        emit({"id": request["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}, "agentInfo": {"version": "fixture"}}})
    elif method == "session/new":
        emit({"id": request["id"], "result": {"sessionId": "fixture-session"}})
    elif method == "session/load":
        emit({"method": "session/update", "params": {"sessionId": "fixture-session", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "OLD HISTORY"}}}})
        emit({"id": request["id"], "result": {}})
    elif method == "session/prompt":
        text = "FIRST" if request["params"]["prompt"][0]["text"]  .startswith("original\n\n") else "SECOND"
        text = "```orbit-delivery\n" + json.dumps({"schemaVersion": 1, "submissionId": "fixture", "items": [{"kind": "markdown", "name": "result.md", "content": text}]}) + "\n```"
        emit({"method": "session/update", "params": {"sessionId": "foreign-session", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "FOREIGN"}}}})
        emit({"method": "session/update", "params": {"sessionId": "fixture-session", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}}})
        emit({"id": request["id"], "result": {"stopReason": "end_turn"}})
