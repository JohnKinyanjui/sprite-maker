#!/usr/bin/env python3
"""Workspace-scoped MCP server for Sprite Studio's deterministic rig engine.

The server intentionally has no third-party dependencies. MCP stdio messages are
newline-delimited JSON-RPC; stdout is reserved exclusively for protocol output.
"""

import argparse
import json
import math
import os
import stat
import subprocess
import sys
from pathlib import Path


SERVER_NAME = "sprite-studio-rig"
SERVER_VERSION = "0.2.0"
LEGACY_PROTOCOL = "2025-11-25"
CURRENT_PROTOCOL = "2026-07-28"
SUPPORTED_PROTOCOLS = [CURRENT_PROTOCOL, LEGACY_PROTOCOL]
INSTRUCTIONS = (
    "Operate only on rig JSON files below this Sprite Studio workspace's "
    ".sprite-studio/rigs directory. Call sprite_rig_validate before rendering. "
    "Use sprite_rig_analyze_motion to inspect IK-resolved contacts, morphology, and "
    "circular transition cadence; never treat a successful subprocess exit alone as visual proof."
)


def parse_args():
    parser = argparse.ArgumentParser(description="Sprite Studio rig MCP server")
    parser.add_argument("--workspace", required=True, help="Bound Sprite Studio workspace")
    return parser.parse_args()


def reject_json_constant(value):
    raise ValueError(f"non-finite JSON constant is not allowed: {value}")


def protocol_error(request_id, code, message, data=None):
    error = {"code": code, "message": message}
    if data is not None:
        error["data"] = data
    return {"jsonrpc": "2.0", "id": request_id, "error": error}


def result_response(request_id, result):
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def modern_request(request):
    params = request.get("params")
    if not isinstance(params, dict):
        return False
    metadata = params.get("_meta")
    return isinstance(metadata, dict) and metadata.get(
        "io.modelcontextprotocol/protocolVersion"
    ) == CURRENT_PROTOCOL


def complete(result, modern):
    if modern:
        return {
            "resultType": "complete",
            "_meta": {
                "io.modelcontextprotocol/serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION,
                }
            },
            **result,
        }
    return result


def tool_definition(name, title, description, read_only, idempotent):
    annotations = {
        "title": title,
        "readOnlyHint": read_only,
        "destructiveHint": False,
        "idempotentHint": idempotent,
        "openWorldHint": False,
    }
    return {
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "rig": {
                    "type": "string",
                    "description": (
                        "Workspace-relative rig JSON under .sprite-studio/rigs/; "
                        "for example .sprite-studio/rigs/hero_walk.json"
                    ),
                }
            },
            "required": ["rig"],
        },
        "annotations": annotations,
    }


TOOLS = [
    tool_definition(
        "sprite_rig_validate",
        "Validate sprite rig",
        "Strictly validate a rig without changing the rig or rendering frames.",
        True,
        True,
    ),
    tool_definition(
        "sprite_rig_render",
        "Render sprite rig",
        (
            "Validate and deterministically render a rig. Writes frames, the active "
            "generation manifest, normalized rig record, and recoverable output history."
        ),
        False,
        False,
    ),
    tool_definition(
        "sprite_rig_analyze_motion",
        "Analyze animation mechanics",
        (
            "Run strict validation and return the anatomy rig profile, observed joint "
            "map, named poses, IK-resolved planted contacts, root-motion mode, "
            "source-bound frame hashes, and per-transition skeletal energy."
        ),
        True,
        True,
    ),
    tool_definition(
        "sprite_rig_analyze_walk",
        "Analyze walk mechanics (legacy)",
        "Compatibility alias for sprite_rig_analyze_motion.",
        True,
        True,
    ),
]


class RigServer:
    def __init__(self, workspace):
        raw_workspace = Path(workspace).expanduser()
        if not raw_workspace.is_dir():
            raise ValueError(f"workspace does not exist: {raw_workspace}")
        self.workspace = raw_workspace.resolve()
        if raw_workspace.is_symlink():
            raise ValueError("workspace cannot be a symbolic link")
        self.rig_root = (self.workspace / ".sprite-studio" / "rigs").resolve()
        self.engine = (self.workspace / ".sprite-studio" / "sprite_rig.py").resolve()
        if not self.rig_root.is_dir() or not self.rig_root.is_relative_to(self.workspace):
            raise ValueError("workspace is missing .sprite-studio/rigs")
        if not self.engine.is_file() or not self.engine.is_relative_to(self.workspace):
            raise ValueError("workspace is missing .sprite-studio/sprite_rig.py")
        self.legacy_state = "new"

    def resolve_rig(self, value):
        if not isinstance(value, str) or not value.strip():
            raise ValueError("rig must be a non-empty workspace-relative JSON path")
        supplied = Path(value)
        if supplied.is_absolute():
            raise ValueError("rig must be workspace-relative, not absolute")
        candidate = (self.workspace / supplied).resolve()
        if not candidate.is_relative_to(self.rig_root):
            raise ValueError("rig must stay under .sprite-studio/rigs")
        if candidate.suffix.lower() != ".json" or not candidate.is_file():
            raise ValueError("rig must be an existing JSON file under .sprite-studio/rigs")
        try:
            opened = candidate.open("rb")
            opened_stat = os.fstat(opened.fileno())
            path_stat = candidate.stat()
        except OSError as error:
            raise ValueError(f"cannot open rig: {error}") from None
        finally:
            if "opened" in locals():
                opened.close()
        if (opened_stat.st_dev, opened_stat.st_ino) != (path_stat.st_dev, path_stat.st_ino):
            raise ValueError("rig changed while its path was being verified")
        if not stat.S_ISREG(opened_stat.st_mode):
            raise ValueError("rig must be a regular JSON file")
        return candidate

    def run_engine(self, rig, render):
        relative = rig.relative_to(self.workspace)
        command = [sys.executable, str(self.engine)]
        if not render:
            command.append("--check")
        command.append(str(relative))
        environment = {key: value for key, value in os.environ.items() if key != "PYTHONPATH"}
        try:
            completed = subprocess.run(
                command,
                cwd=self.workspace,
                env=environment,
                capture_output=True,
                text=True,
                timeout=120,
                check=False,
            )
        except subprocess.TimeoutExpired:
            raise ValueError("rig engine timed out after 120 seconds") from None
        stdout = completed.stdout.strip()
        stderr = completed.stderr.strip()
        if completed.returncode != 0:
            message = stderr or stdout or f"rig engine exited {completed.returncode}"
            raise ValueError(message)
        if stderr:
            raise ValueError(f"rig engine wrote unexpected diagnostics: {stderr}")
        try:
            payload = json.loads(stdout, parse_constant=reject_json_constant)
        except (json.JSONDecodeError, ValueError):
            raise ValueError("rig engine returned invalid JSON") from None
        if not isinstance(payload, dict):
            raise ValueError("rig engine returned a non-object result")
        return payload

    @staticmethod
    def analyze(report):
        if not isinstance(report.get("rigVersion"), int) or report["rigVersion"] < 2:
            raise ValueError("motion analysis requires a rigVersion 2 or newer articulated rig")
        analysis = report.get("analysis")
        if not isinstance(analysis, dict):
            raise ValueError("validated rig did not return solved motion analysis")
        energies = analysis.get("transitionEnergyPx")
        contacts = analysis.get("plantedContacts")
        phases = analysis.get("phases")
        poses = analysis.get("poses", [])
        joints = analysis.get("joints", [])
        if not isinstance(energies, list) or not energies:
            raise ValueError("validated rig has no solved transition-energy series")
        if not isinstance(contacts, list) or not contacts:
            raise ValueError("validated rig has no solved planted contacts")
        if not isinstance(phases, list) or not all(isinstance(value, str) and value for value in phases):
            raise ValueError("validated rig has incomplete gait phases")
        if not isinstance(poses, list) or not all(isinstance(value, str) for value in poses):
            raise ValueError("validated rig returned invalid named poses")
        if not isinstance(joints, list) or not all(isinstance(value, dict) for value in joints):
            raise ValueError("validated rig returned an invalid observed-joint map")
        master_hash = report.get("masterHash")
        rig_hash = report.get("rigHash")
        frame_hashes = report.get("quality", {}).get("renderedFrameHashes")
        if not all(
            isinstance(value, str)
            and len(value) == 64
            and all(character in "0123456789abcdef" for character in value)
            for value in (master_hash, rig_hash)
        ):
            raise ValueError("validated rig did not return source and rig identity hashes")
        if (
            not isinstance(frame_hashes, list)
            or len(frame_hashes) != report.get("frames")
            or not all(
                isinstance(value, str)
                and len(value) == 64
                and all(character in "0123456789abcdef" for character in value)
                for value in frame_hashes
            )
        ):
            raise ValueError("validated rig did not return one raster hash per frame")
        numeric = [float(value) for value in energies]
        if not all(math.isfinite(value) and value >= 0 for value in numeric):
            raise ValueError("validated rig returned invalid transition energies")
        positive = [value for value in numeric if value > 1e-9]
        cadence_ratio = max(positive) / min(positive) if positive else None
        contact_groups = {}
        for contact in contacts:
            if not isinstance(contact, dict):
                raise ValueError("validated rig returned an invalid planted contact")
            key = f"{contact.get('part')}.{contact.get('anchor')}"
            contact_groups.setdefault(key, []).append({
                "frame": contact.get("frame"),
                "x": contact.get("x"),
                "y": contact.get("y"),
            })
        return {
            "valid": report.get("valid") is True,
            "name": report.get("name"),
            "masterHash": master_hash,
            "rigHash": rig_hash,
            "rigVersion": report.get("rigVersion"),
            "frameCount": report.get("frames"),
            "uniqueRenderedFrames": report.get("quality", {}).get("uniqueRenderedFrames"),
            "renderedFrameHashes": frame_hashes,
            "looping": analysis.get("looping"),
            "rootMotion": analysis.get("rootMotion"),
            "rigProfile": analysis.get("rigProfile"),
            "joints": joints,
            "visibleJointCount": sum(
                1 for joint in joints if joint.get("visibility") == "visible"
            ),
            "phases": phases,
            "poses": poses,
            "transitionEnergyPx": numeric,
            "cadenceRatio": round(cadence_ratio, 6) if cadence_ratio is not None else None,
            "plantedContactGroups": contact_groups,
            "warnings": report.get("warnings", []),
        }

    def call_tool(self, name, arguments):
        if not isinstance(arguments, dict):
            raise ValueError("tool arguments must be an object")
        if set(arguments) != {"rig"}:
            raise ValueError("tool arguments require only the rig field")
        rig = self.resolve_rig(arguments["rig"])
        if name == "sprite_rig_validate":
            return self.run_engine(rig, render=False)
        if name == "sprite_rig_render":
            return self.run_engine(rig, render=True)
        if name in {"sprite_rig_analyze_motion", "sprite_rig_analyze_walk"}:
            return self.analyze(self.run_engine(rig, render=False))
        raise KeyError(name)

    def handle(self, request):
        if not isinstance(request, dict) or request.get("jsonrpc") != "2.0":
            return protocol_error(None, -32600, "Invalid Request")
        method = request.get("method")
        if "id" not in request:
            if method == "notifications/initialized":
                if self.legacy_state == "initializing":
                    self.legacy_state = "ready"
                return None
            return None
        request_id = request["id"]
        if request_id is None or isinstance(request_id, bool) or not isinstance(request_id, (str, int)):
            return protocol_error(None, -32600, "request id must be a string or integer")
        if not isinstance(method, str) or not method:
            return protocol_error(request_id, -32600, "request method must be a non-empty string")
        modern = modern_request(request)
        params = request.get("params")
        metadata = params.get("_meta", {}) if isinstance(params, dict) else {}
        requested_protocol = metadata.get("io.modelcontextprotocol/protocolVersion")
        if requested_protocol is not None and requested_protocol not in SUPPORTED_PROTOCOLS:
            return protocol_error(
                request_id,
                -32022,
                f"Unsupported protocol version: {requested_protocol}",
                {"supported": SUPPORTED_PROTOCOLS, "requested": requested_protocol},
            )
        if method == "initialize":
            if self.legacy_state != "new":
                return protocol_error(request_id, -32600, "server is already initialized")
            params = request.get("params", {})
            if not isinstance(params, dict):
                return protocol_error(request_id, -32602, "initialize params must be an object")
            requested = params.get("protocolVersion")
            if not isinstance(requested, str):
                return protocol_error(request_id, -32602, "initialize requires protocolVersion")
            if not isinstance(params.get("capabilities"), dict):
                return protocol_error(request_id, -32602, "initialize requires capabilities")
            client_info = params.get("clientInfo")
            if not isinstance(client_info, dict) or not all(
                isinstance(client_info.get(key), str) and client_info.get(key)
                for key in ("name", "version")
            ):
                return protocol_error(request_id, -32602, "initialize requires clientInfo name and version")
            protocol = requested if requested in SUPPORTED_PROTOCOLS else CURRENT_PROTOCOL
            self.legacy_state = "initializing"
            return result_response(request_id, {
                "protocolVersion": protocol,
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "instructions": INSTRUCTIONS,
            })
        if method == "server/discover":
            if self.legacy_state != "new" and not modern:
                return protocol_error(request_id, -32600, "cannot mix discovery with legacy initialization")
            return result_response(request_id, complete({
                "supportedVersions": SUPPORTED_PROTOCOLS,
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "instructions": INSTRUCTIONS,
                "ttlMs": 300000,
                "cacheScope": "public",
            }, True))
        if method == "ping":
            return result_response(request_id, complete({}, modern))
        if not modern and self.legacy_state != "ready":
            return protocol_error(request_id, -32003, "MCP server is not initialized")
        if method == "tools/list":
            result = {"tools": TOOLS}
            if modern:
                result.update({"ttlMs": 300000, "cacheScope": "public"})
            return result_response(request_id, complete(result, modern))
        if method == "tools/call":
            params = request.get("params")
            if not isinstance(params, dict) or not isinstance(params.get("name"), str):
                return protocol_error(request_id, -32602, "Invalid tool call parameters")
            name = params["name"]
            if name not in {tool["name"] for tool in TOOLS}:
                return protocol_error(request_id, -32602, f"Unknown tool: {name}")
            try:
                structured = self.call_tool(name, params.get("arguments", {}))
                result = {
                    "content": [{"type": "text", "text": json.dumps(structured, sort_keys=True)}],
                    "structuredContent": structured,
                    "isError": False,
                }
            except (OSError, ValueError) as error:
                structured = {"ok": False, "error": str(error)}
                result = {
                    "content": [{"type": "text", "text": str(error)}],
                    "structuredContent": structured,
                    "isError": True,
                }
            return result_response(request_id, complete(result, modern))
        return protocol_error(request_id, -32601, f"Method not found: {method}")


def main():
    try:
        args = parse_args()
        server = RigServer(args.workspace)
    except (OSError, ValueError) as error:
        print(f"sprite_rig_mcp: {error}", file=sys.stderr)
        raise SystemExit(2) from None
    for line in sys.stdin:
        try:
            request = json.loads(line, parse_constant=reject_json_constant)
            if isinstance(request, list):
                raise ValueError("JSON-RPC batches are not supported by this server")
            response = server.handle(request)
        except json.JSONDecodeError as error:
            response = protocol_error(None, -32700, f"Parse error: {error.msg}")
        except ValueError as error:
            response = protocol_error(None, -32600, str(error))
        except Exception as error:  # Protocol containment: never corrupt stdout or crash the server.
            response = protocol_error(None, -32603, "Internal error", str(error))
        if response is not None:
            sys.stdout.write(
                json.dumps(response, separators=(",", ":"), allow_nan=False) + "\n"
            )
            sys.stdout.flush()


if __name__ == "__main__":
    main()
