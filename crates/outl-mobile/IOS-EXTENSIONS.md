# iOS app extensions (RFC 0363)

Capture from outside the app: a widget, an App Intent ("Add to today",
which Shortcuts, Siri and the Action button use) and a share extension.
Design and invariants: [`docs/rfcs/0363-ios-quick-capture.md`](../../docs/rfcs/0363-ios-quick-capture.md).

## What lives where

| Piece | Path | Target |
|---|---|---|
| Widget (opens `outl://capture`) | `src-tauri/gen/apple/OutlWidget/` | `OutlWidget` app extension, iOS 16+ |
| Shared pure logic (links, inbox writer) | `swift/OutlKit/Sources/OutlKit/Capture/` | OutlKit package, unit tested by `swift test` |

The extension targets were added to the committed `project.pbxproj` by
`scripts/ios-add-extension.py`; `project.yml` mirrors them for anyone who
regenerates with `xcodegen`.

## What CI checks and what it cannot

- `swift (OutlKit unit tests)` runs on every PR: the pure Swift pieces.
- `iOS Simulator build (opt-in)` (run it from the Actions tab, or label the
  PR `ios-build`) compiles the app with its extensions for the simulator,
  unsigned.
- The release job signs each extension with its own profile, or, when a
  profile secret is missing, unhooks the extensions and ships the app
  alone (`scripts/ios-extensions-signing.py`).

## What needs Xcode on a Mac

1. **Apple Developer portal.** Register the App IDs
   `app.outl.mobile-app.widget` (and, from step 3, `app.outl.mobile-app.share`).
2. **App Store profiles.** One per extension. Add them as repository
   secrets `WIDGET_PROVISIONING_PROFILE_BASE64` and
   `SHARE_PROVISIONING_PROFILE_BASE64` (base64 of the `.mobileprovision`).
   Until both exist, release builds ship without the extensions.
3. **Run on a device.** Open `src-tauri/gen/apple/outl-mobile.xcodeproj`,
   pick your team on each extension target (Signing & Capabilities), run
   the `outl-mobile_iOS` scheme on a phone, then add the "Add to today"
   widget from the Home Screen and the Lock Screen and tap it: the app
   should open on the capture sheet.
