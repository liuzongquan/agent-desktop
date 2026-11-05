#!/usr/bin/env python3
"""Exercise the built CLI, MCP and C ABI against a stateful W3C/Appium fixture.

This checks protocol integration, not a device or Appium driver installation.
Run after cargo build -p agent-desktop -p agent-desktop-ffi:
    python3 tests/e2e-mobile/run.py
"""

import argparse
import base64
import ctypes
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import threading
import time
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from xml.sax.saxutils import quoteattr


ELEMENT_KEY = "element-6066-11e4-a52e-4f735466cecf"
def screenshot_fixture():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    header = struct.pack(">IIBBBBB", 400, 800, 8, 2, 0, 0, 0)
    pixels = (b"\x00" + b"\x40\x80\xc0" * 400) * 800
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
    png += chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")
    return base64.b64encode(png).decode()


PNG = screenshot_fixture()


class Device:
    def __init__(self, platform):
        self.platform = platform
        self.package = "org.example.mobilefixture"
        self.text = ""
        self.clipboard = ""
        self.checked = False
        self.label = "Submit"
        self.duplicates = False
        self.context = "NATIVE_APP"
        self.source_delay = 0
        self.fail_click = None
        self.fail_actions = False
        self.wrong_handle = False
        self.switch_during_element_read = False
        self.switch_on_source = False
        self.switch_on_screenshot = False
        self.requests = []
        self.writes = []
        self.errors = []

    def source(self):
        if self.source_delay:
            time.sleep(self.source_delay)
        elements = []
        for kind, key, label, value, y in [
            ("Button", "submit", self.label, self.label, 20),
            ("EditText", "message", "Message", self.text, 80),
            ("Switch", "wifi", "Wi-Fi", "1" if self.checked else "0", 140),
        ]:
            if self.platform == "android":
                attrs = {
                    "class": "android.widget." + kind,
                    "resource-id": "org.example:id/" + key,
                    "content-desc": label,
                    "text": value,
                    "bounds": f"[10,{y}][210,{y + 40}]",
                    "enabled": "true", "displayed": "true", "clickable": "true",
                    "checked": str(self.checked and key == "wifi").lower(),
                }
                tag = "node"
            else:
                tag = "XCUIElementType" + ("TextField" if kind == "EditText" else kind)
                attrs = {"type": tag, "name": key, "label": label, "value": value,
                         "enabled": "true", "visible": "true", "x": "10",
                         "y": str(y), "width": "200", "height": "40"}
            element = "<" + tag + " " + " ".join(
                name + "=" + quoteattr(value) for name, value in attrs.items()) + "/>"
            elements.append(element)
            if key == "submit" and self.duplicates:
                elements.append(element)
        root = "hierarchy" if self.platform == "android" else "AppiumAUT"
        return f"<{root}>" + "".join(elements) + f"</{root}>"

    def request(self, method, route, body):
        self.requests.append((method, route, body))
        if route == "" and method == "GET":
            return {"platformName": self.platform, "appium:automationName":
                    "UiAutomator2" if self.platform == "android" else "XCUITest"}
        if route == "/context":
            return self.context
        if route == "/appium/device/current_package":
            return self.package
        if route == "/window/rect":
            return {"x": 0, "y": 0, "width": 400, "height": 800}
        if route == "/source":
            source = self.source()
            if self.switch_on_source:
                self.switch_on_source = False
                self.package = "org.example.interruptingapp"
            return source
        if route == "/screenshot":
            if self.switch_on_screenshot:
                self.switch_on_screenshot = False
                self.package = "org.example.interruptingapp"
            return PNG
        if route == "/appium/device/set_clipboard":
            self.clipboard = body["content"]
            return None
        if route == "/appium/device/get_clipboard":
            return self.clipboard
        if route == "/elements":
            assert body["using"] == "xpath", body
            xpath = body["value"]
            key = next((key for key in ["submit", "message", "wifi"] if key in xpath), None)
            assert key, xpath
            return [{ELEMENT_KEY: key}] * (2 if key == "submit" and self.duplicates else 1)
        match = re.fullmatch(r"/element/([^/]+)/(.*)", route)
        if match:
            key, operation = match.groups()
            if method == "GET" and operation.startswith("attribute/"):
                attr = operation[len("attribute/"):]
                kind = {"submit": "Button", "message": "EditText", "wifi": "Switch"}[key]
                label = {"submit": self.label, "message": "Message", "wifi": "Wi-Fi"}[key]
                native_type = "XCUIElementType" + ("TextField" if kind == "EditText" else kind)
                values = {"class": "android.widget." + kind, "type": native_type,
                          "resource-id": "org.example:id/" + key, "name": key,
                          "content-desc": label, "label": label,
                          "text": self.text if key == "message" else label}
                if self.wrong_handle and attr in ("name", "resource-id"):
                    return "a-different-native-element"
                assert attr in values, attr
                return values[attr]
            if method == "GET" and operation == "enabled":
                if self.switch_during_element_read:
                    self.switch_during_element_read = False
                    self.switch_on_source = True
                return True
            if method == "GET" and operation == "rect":
                return {"x": 10, "y": 20, "width": 200, "height": 40}
            if method == "POST" and operation == "click":
                if self.fail_click:
                    return {"error": self.fail_click, "message": "fixture refusal"}
                self.writes.append((key, operation, body))
                if key == "wifi":
                    self.checked = not self.checked
                return None
            if method == "POST" and operation in ("value", "clear"):
                assert key == "message", key
                self.writes.append((key, operation, body))
                self.text = "" if operation == "clear" else self.text + body["text"]
                return None
        if route == "/execute/sync":
            script = body["script"]
            if script == "mobile: activeAppInfo":
                return {"bundleId": self.package, "pid": 123}
            if script == "mobile: setClipboard":
                self.clipboard = body["args"][0]["content"]
                return None
            if script == "mobile: getClipboard":
                return self.clipboard
            if script in ("mobile: doubleClickGesture", "mobile: doubleTap"):
                assert body["args"][0]["elementId"] == "submit"
                self.writes.append(("submit", "double-click", body))
                return None
            if script == "mobile: pressKey":
                self.writes.append(("key", script, body["args"][0]))
                return None
            if script in ("mobile: activateApp", "mobile: terminateApp", "mobile: queryAppState"):
                args = body["args"][0]
                if script.endswith("activateApp"):
                    self.package = args.get("appId", args.get("bundleId"))
                    self.writes.append(("app", script, args))
                    return None
                if script.endswith("terminateApp"):
                    self.package = "org.example.home"
                    self.writes.append(("app", script, args))
                    return True
                return 4
        if route == "/actions" and method in ("POST", "DELETE"):
            self.writes.append(("touch", method, body))
            if method == "POST" and self.fail_actions:
                return {"error": "timeout", "message": "fixture partial delivery"}
            return None
        raise AssertionError(f"Unexpected request: {method} {route} {body}")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def handle_request(self):
        length = int(self.headers.get("Content-Length", "0"))
        body = json.loads(self.rfile.read(length)) if length else None
        match = re.fullmatch(r"/session/([^/]+)(.*)", self.path)
        try:
            assert match, self.path
            result = self.server.device.request(self.command, match[2], body)
            status = 404 if isinstance(result, dict) and result.get("error") else 200
        except Exception as error:
            self.server.device.errors.append(str(error))
            result = {"error": "unknown command", "message": str(error)}
            status = 404
        data = json.dumps({"value": result}).encode()
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    do_GET = do_POST = do_DELETE = handle_request


def nodes(tree):
    yield tree
    for child in tree.get("children", []):
        yield from nodes(child)


class Scenario:
    def __init__(self, binary, library, platform, state_root, server):
        self.binary, self.library = binary, library
        self.device, self.platform = server.device, platform
        self.env = dict(os.environ, AGENT_DESKTOP_PLATFORM=platform,
                        AGENT_DESKTOP_APPIUM_URL=f"http://127.0.0.1:{server.server_port}",
                        AGENT_DESKTOP_APPIUM_SESSION_ID="fixture-session",
                        AGENT_DESKTOP_HOME=str(state_root))
        self.count = 0

    def cli(self, *args, error=None, env=None):
        result = subprocess.run([str(self.binary), *args], capture_output=True, text=True,
                                env=env or self.env, timeout=12)
        try:
            envelope = json.loads(result.stdout)
        except ValueError:
            raise AssertionError((args, result.returncode, result.stdout, result.stderr))
        assert envelope["version"] == "2.6", envelope
        assert envelope["ok"] == (error is None), (args, envelope, result.stderr)
        assert result.returncode == (0 if error is None else 1), (args, result)
        if error:
            assert envelope["error"]["code"] in error.split("|"), (args, envelope)
        self.count += 1
        return envelope

    def run(self):
        assert self.cli("status")["data"]["platform"] == self.platform
        skill = self.cli("skills", "get", "platform")["data"]["content"]
        assert "Appium" in skill and self.platform.lower() in skill.lower()
        snapshot = self.cli("snapshot", "--app", self.device.package)["data"]
        assert snapshot["complete"] is True
        refs = {node["name"]: node["ref_id"] for node in nodes(snapshot["tree"])
                if node.get("ref_id") and node.get("name")}
        assert {"Submit", "Message", "Wi-Fi"} <= refs.keys(), snapshot
        self.cli("click", refs["Submit"])
        assert sum(item[0:2] == ("submit", "click") for item in self.device.writes) == 1
        self.cli("--headed", "double-click", refs["Submit"])
        assert sum(item[0:2] == ("submit", "double-click") for item in self.device.writes) == 1
        before_focus = len(self.device.writes)
        self.cli("focus", refs["Submit"], error="ACTION_NOT_SUPPORTED|PLATFORM_NOT_SUPPORTED")
        assert len(self.device.writes) == before_focus
        self.cli("type", refs["Message"], "你好 mobile")
        assert self.device.text == "你好 mobile"
        self.cli("set-value", refs["Message"], "replacement")
        assert self.device.text == "replacement"
        self.cli("clear", refs["Message"])
        assert self.device.text == ""
        self.cli("check", refs["Wi-Fi"])
        assert self.device.checked
        previous = len(self.device.writes)
        self.cli("check", refs["Wi-Fi"])
        assert len(self.device.writes) == previous
        self.cli("uncheck", refs["Wi-Fi"])
        assert not self.device.checked
        image = self.cli("screenshot")["data"]
        assert base64.b64decode(image["data"]) == base64.b64decode(PNG)
        assert image["width"] == 400 and image["height"] == 800
        self.cli("snapshot", "--root", refs["Message"])
        self.cli("snapshot", "--max-depth", "0")
        self.cli("clipboard-set", "mobile 剪贴板")
        assert base64.b64decode(self.device.clipboard).decode() == "mobile 剪贴板"
        assert self.cli("clipboard-get")["data"]["text"] == "mobile 剪贴板"
        self.cli("clipboard-clear")
        assert self.cli("clipboard-get")["data"]["text"] == ""
        self.cli("--headed", "mouse-click", "--xy", "25,30")
        touch = next(item[2] for item in reversed(self.device.writes) if item[:2] == ("touch", "POST"))
        assert touch["actions"][0]["parameters"]["pointerType"] == "touch"
        assert touch["actions"][0]["actions"][0]["x"] == 25
        self.cli("--headed", "drag", "--from-xy", "20,200", "--to-xy", "20,40")
        touch = next(item[2] for item in reversed(self.device.writes) if item[:2] == ("touch", "POST"))
        assert any(action.get("y") == 40 for action in touch["actions"][0]["actions"])
        self.cli("press", "enter", "--app", self.device.package)
        keys = self.device.writes[-1][2]["actions"][0]
        assert keys["type"] == "key"
        assert keys["actions"] == [{"type": "keyDown", "value": "\ue007"},
                                    {"type": "keyUp", "value": "\ue007"}]
        if self.platform == "android":
            self.cli("press", "back")
            assert self.device.writes[-1][2] == {"keycode": 4}
        before_failure = len(self.device.writes)
        self.device.fail_actions = True
        failure = self.cli("--headed", "mouse-click", "--xy", "25,30", error="TIMEOUT")
        self.device.fail_actions = False
        assert failure["error"]["disposition"]["retry"] != "safe"
        assert [entry[1] for entry in self.device.writes[before_failure:]] == ["POST", "DELETE"]
        before = len(self.device.writes)
        self.device.label = "Changed"
        self.cli("click", refs["Submit"], error="STALE_REF")
        self.device.label = "Submit"
        self.device.duplicates = True
        self.cli("click", refs["Submit"], error="AMBIGUOUS_TARGET")
        self.device.duplicates = False
        other = dict(self.env, AGENT_DESKTOP_APPIUM_SESSION_ID="another-session")
        self.cli("click", refs["Submit"], env=other, error="STALE_REF|WINDOW_NOT_FOUND")
        assert len(self.device.writes) == before
        self.device.fail_click = "stale element reference"
        self.cli("click", refs["Submit"], error="STALE_REF")
        self.device.fail_click = None
        self.device.wrong_handle = True
        self.cli("click", refs["Submit"], error="STALE_REF")
        self.device.wrong_handle = False
        assert len(self.device.writes) == before
        original = self.device.package
        self.device.switch_during_element_read = True
        self.cli("click", refs["Submit"], error="STALE_REF|WINDOW_NOT_FOUND")
        assert self.device.package == "org.example.interruptingapp"
        assert len(self.device.writes) == before
        self.device.package = original
        self.device.switch_on_screenshot = True
        self.cli("screenshot", "--app", original, error="STALE_REF")
        assert self.device.package == "org.example.interruptingapp"
        self.device.package = original
        self.device.context = "WEBVIEW_1"
        self.cli("snapshot", error="ACTION_NOT_SUPPORTED")
        self.device.context = "NATIVE_APP"
        self.cli("minimize", "--app", self.device.package,
                 error="PLATFORM_NOT_SUPPORTED|ACTION_NOT_SUPPORTED")
        self.device.source_delay = 0.5
        start = time.monotonic()
        self.cli("snapshot", "--timeout-ms", "100", error="TIMEOUT")
        assert time.monotonic() - start < 1.5
        self.device.source_delay = 0
        self.lifecycle()
        self.mcp()
        if self.library:
            self.ffi()
        assert not self.device.errors, self.device.errors

    def lifecycle(self):
        original = self.device.package
        before = len(self.device.writes)
        self.cli("launch", original, "--no-attach", "--activate", error="ACTION_FAILED")
        assert len(self.device.writes) == before
        self.cli("launch", "org.example.secondfixture", error="PLATFORM_NOT_SUPPORTED")
        assert len(self.device.writes) == before
        assert self.device.package == original
        self.cli("launch", "org.example.secondfixture", "--activate")
        assert self.device.package == "org.example.secondfixture"
        before = len(self.device.writes)
        self.cli("close-app", self.device.package, error="PLATFORM_NOT_SUPPORTED")
        assert len(self.device.writes) == before
        assert self.device.package == "org.example.secondfixture"
        self.cli("close-app", self.device.package, "--force")
        assert self.device.package == "org.example.home"
        self.cli("launch", original, "--activate")
        assert self.device.package == original
        for change in [
            {"AGENT_DESKTOP_PLATFORM": "unknown"},
            {"AGENT_DESKTOP_APPIUM_SESSION_ID": "../other"},
            {"AGENT_DESKTOP_APPIUM_URL": "http://user:password@localhost:4723"},
        ]:
            before = len(self.device.requests)
            self.cli("status", env=dict(self.env, **change), error="INVALID_ARGS")
            assert len(self.device.requests) == before

    def mcp(self):
        request = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                   "params": {"name": "desktop_snapshot", "arguments": {"app": self.device.package}}}
        result = subprocess.run([str(self.binary), "mcp"], input=json.dumps(request) + "\n",
                                capture_output=True, text=True, env=self.env, timeout=12)
        assert result.returncode == 0, result.stderr
        response = json.loads(result.stdout)["result"]
        assert not response["isError"], response
        assert response["structuredContent"]["data"]["complete"] is True, response
        self.count += 1

    def ffi(self):
        before = dict(os.environ)
        os.environ.update(self.env)
        lib = ctypes.CDLL(str(self.library))
        lib.ad_init.argtypes = [ctypes.c_uint32]
        lib.ad_adapter_create.restype = ctypes.c_void_p
        lib.ad_adapter_destroy.argtypes = [ctypes.c_void_p]
        lib.ad_free_string.argtypes = [ctypes.c_void_p]
        lib.ad_status.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
        lib.ad_snapshot.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int32,
                                    ctypes.c_uint8, ctypes.c_bool, ctypes.c_bool,
                                    ctypes.POINTER(ctypes.c_void_p)]
        assert lib.ad_init(7) == 0
        adapter = lib.ad_adapter_create()
        assert adapter
        try:
            output = ctypes.c_void_p()
            assert lib.ad_status(adapter, ctypes.byref(output)) == 0
            status = json.loads(ctypes.string_at(output))
            lib.ad_free_string(output)
            assert status["data"]["platform"] == self.platform, status
            output = ctypes.c_void_p()
            assert lib.ad_snapshot(adapter, self.device.package.encode(), 0, 50, False,
                                   False, ctypes.byref(output)) == 0
            snapshot = json.loads(ctypes.string_at(output))
            lib.ad_free_string(output)
            assert snapshot["ok"] and snapshot["data"]["complete"], snapshot
            self.count += 2
        finally:
            lib.ad_adapter_destroy(adapter)
            os.environ.clear()
            os.environ.update(before)


def main():
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=root / "target/debug/agent-desktop")
    parser.add_argument("--library", type=Path)
    args = parser.parse_args()
    if args.library is None:
        args.library = next((root / "target/debug" / name for name in [
            "libagent_desktop_ffi.dylib", "libagent_desktop_ffi.so", "agent_desktop_ffi.dll"]
            if (root / "target/debug" / name).is_file()), None)
    start = time.monotonic()
    results = {}
    with tempfile.TemporaryDirectory(prefix="agent-desktop-mobile-") as temporary:
        for platform in ("android", "ios"):
            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.device = Device(platform)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                scenario = Scenario(args.binary.resolve(), args.library, platform,
                                    Path(temporary) / platform, server)
                scenario.run()
                results[platform] = {"checks": scenario.count,
                                     "protocol_requests": len(server.device.requests)}
            finally:
                server.shutdown()
                server.server_close()
                thread.join()
    print(json.dumps({"ok": True, "kind": "protocol_fixture", "platforms": results,
                      "ffi": args.library is not None,
                      "elapsed_seconds": round(time.monotonic() - start, 3)}))


if __name__ == "__main__":
    main()
