---
name: agent-desktop-mobile
description: Operate Android and iOS apps through an existing native Appium session with the unchanged agent-desktop CLI, MCP tools, and C ABI.
requirements:
  - agent-desktop
  - Appium with UiAutomator2 for Android or XCUITest for iOS
---

# Android and iOS

Read this with `agent-desktop skills get desktop`. The mobile adapter uses the same commands, qualified refs, JSON envelopes, and retry rules as the desktop adapters. Appium runs on the host connected to the device; agent-desktop may connect from another host.

## Connect to a device

Configure an existing native Appium session before running device commands:

```bash
export AGENT_DESKTOP_PLATFORM=android
export AGENT_DESKTOP_APPIUM_URL=http://127.0.0.1:4723
export AGENT_DESKTOP_APPIUM_SESSION_ID=your_existing_appium_session_id
agent-desktop status
agent-desktop snapshot -i
```

Replace the session ID placeholder. Use `ios` for an XCUITest session. Android uses UiAutomator2 and an authorized device or emulator. XCUITest needs macOS, Xcode, and a configured simulator or signed WebDriverAgent on a development device. See the repository's Android and iOS setup page for session creation examples.

The default URL is `http://127.0.0.1:4723`; include a custom Appium base path if needed. Keep the session in `NATIVE_APP` context. agent-desktop attaches to the session and does not create or delete it. The Appium session ID is separate from `--session`: the latter isolates saved agent-desktop snapshots and traces, and `session start` does not connect a device.

MCP command processes inherit these variables from the MCP server. C callers set them before creating an adapter. Unset `AGENT_DESKTOP_PLATFORM`, or set it to `desktop`, to return to the host desktop.

## Observe and verify

1. Run `status` and inspect `ok`, permissions, and supported surfaces.
2. Run `snapshot -i` or `find` and copy the returned qualified ref.
3. Act on that ref, then observe the device again to verify the result.
4. Take fresh refs after switching the Appium endpoint or session.

The mobile adapter exposes the current app through the `window` surface. Appium actions affect the device UI and may change focus. Do not assume desktop background-input guarantees on a phone. Keep other Appium clients from changing the UI during a command.

A ref action re-identifies the control before input. A missing control returns `STALE_REF`; multiple matching controls return `AMBIGUOUS_TARGET`. Never retry input after uncertain delivery. Read the live state first, as described in the core skill.

## Command support

| Group | Mobile behavior |
| --- | --- |
| `snapshot`, `find`, `get`, `is`, `wait` | Read the current native app tree; use the `window` surface. |
| `click`, `double-click`, `type`, `set-value`, `clear`, `focus` | Dispatch native Appium element operations. `focus` is limited to text fields. `double-click` requires `--headed`. Input can change device focus. |
| `toggle`, `check`, `uncheck` | Use the control's exposed checked state. Re-observe afterward. |
| `scroll` | Scroll the referenced container using the platform gesture implementation. |
| `scroll-to` | Succeeds only for an already visible element. Scroll its container and take a fresh snapshot to reach hidden content. |
| `list-apps`, `list-windows` | Return the foreground app and its device window, with synthetic IDs scoped to the endpoint, session, and app. They do not enumerate all installed or background apps. |
| `launch`, `close-app` | Use the Android package ID or iOS bundle ID. Launching a different app requires `--activate`. Closing requires `--force`, because Appium provides termination, not graceful close. |
| `screenshot`, `list-displays` | Expose one device display. Capture the screen or the current app's exact window; PNG pixel dimensions may differ from touch coordinates. |
| `clipboard-get`, `clipboard-set`, `clipboard-clear` | Text through Appium, subject to device and driver permissions. |
| `press` | Send a character or Enter, Tab, Backspace, Delete, Esc, Space, and arrow keys through WebDriver. Android also accepts Back, Home, and AppSwitch. A ref target must already be the focused text field. |
| `mouse-click`, `drag` | Map to touch input and swipes. Pass `--headed`; coordinates are Appium device coordinates, not host screen pixels. |

Mobile process IDs identify the Appium endpoint, session, and app; they are not OS process IDs. Take fresh snapshots after app launch, termination, or restart. To isolate a restarted app from all earlier refs, start a new Appium session.

## Platform limits

Window resizing, moving, minimizing, maximizing, and restoring, notifications, desktop system surfaces, `right-click`, `triple-click`, `select`, `expand`, `collapse`, pointer hover, and held-input commands do not have mobile implementations. They keep their command names and return structured unsupported errors. Do not translate desktop right-click or keyboard shortcuts into arbitrary mobile gestures.

Ref operations dispatch to the device by element ID. Most do not require `--headed`; `double-click` requires it under the existing command policy. Raw coordinate input requires `--headed` under the existing CLI policy. Neither mode moves the host cursor.

Appium session expiration requires a new session and a fresh snapshot. A configured platform that does not match the Appium session is an error; do not silently switch targets.
