---
title: "feat: Manage MPD output devices via config fragments"
type: feat
date: 2026-07-18
origin: ce-plan-bootstrap
---

# feat: Manage MPD output devices via config fragments

## Goal Capsule

- **Objective:** Let the admin add, edit, and remove MPD audio output devices from Settings > Playback devices, backed by managed MPD config fragments. Existing runtime enable/disable toggles persist.
- **Authority:** ce-plan owns planning. ce-work owns implementation.
- **Stop conditions:** Device config CRUD works end-to-end. MPD restarts pick up new config fragments. Include directive is auto-injected into MPD config. All existing tests pass.
- **Tail ownership:** Back to the user at the post-generation menu.

---

## Product Contract

### Summary

Add a persistent device-config management layer to the Settings > Playback devices section. The backend writes `audio_output {}` blocks as individual config fragment files in `<data_dir>/mpd-outputs.d/`, auto-injects an `include` directive into MPD's config file on first write, and exposes CRUD API endpoints. The frontend DevicesView gains add/edit/remove workflows and a restart-pending banner. MPD restart is user-triggered from the UI to avoid disrupting active playback.

### Problem Frame

The existing DevicesView lists MPD's runtime outputs and lets the admin enable or disable them, but it cannot create new outputs — MPD's protocol has no `addoutput` command. Audio output definitions live in `mpd.conf` as `audio_output {}` blocks, which the admin must currently edit by hand on the server. This creates friction: the admin has to SSH in, edit a config file, and restart MPD, then return to the browser. A web-managed config fragment layer closes that gap without requiring MPD protocol changes.

### Requirements

**Device config fragments**

- R1. Output device configs are persisted as individual `.conf` files in `<data_dir>/mpd-outputs.d/`, one file per device.
- R2. Each fragment contains a single MPD `audio_output {}` block with the required `type` and `name` fields and optional `device`, `format`, `mixer_type`, `mixer_device`, and `dop` fields.
- R3. Fragment files are named `<sanitized-device-name>.conf` and are safe for MPD's `include` glob pattern.

**API surface**

- R4. `POST /api/devices/configs` creates a new device config fragment. Validates required fields and MPD syntax before writing.
- R5. `GET /api/devices/configs` lists all managed device config fragments (name, type, device, enabled-in-MPD status).
- R6. `PUT /api/devices/configs/{name}` updates an existing device config fragment.
- R7. `DELETE /api/devices/configs/{name}` removes a device config fragment.
- R8. `POST /api/devices/restart-mpd` triggers an MPD restart and returns when MPD is reachable again.
- R9. The existing `GET /api/devices` and enable/disable endpoints continue unchanged — they reflect runtime MPD state.

**Include directive auto-injection**

- R10. When the first device config is created (if `mpd_config` is set in Oxide's config), the backend reads `mpd_config`, checks for an existing `include` of the fragment directory, and adds `include "/abs/path/to/mpd-outputs.d/*.conf"` before the first `audio_output {}` block if absent. The modified file is written atomically.
- R11. If `mpd_config` is not set, the backend stores the fragment files but surfaces a notice that the include directive must be added manually to MPD's main config.

**Frontend**

- R12. DevicesView renders the runtime device list (enable/disable toggles) and a separate list of managed configs with create/edit/remove actions.
- R13. "Add device" form collects type, name, device path, format, mixer_type, and dop toggle.
- R14. Edit form is pre-populated from the existing config fragment.
- R15. "Remove device" removes the config fragment and shows a confirmation dialog.
- R16. A restart-pending banner appears when device configs have been created, edited, or removed but MPD has not been restarted since the change.
- R17. The existing ConfigView "Playback devices" card is preserved but links to the enhanced DevicesView.

### Scope Boundaries

**In scope:** MPD config fragment CRUD, include directive injection, user-triggered MPD restart, enhanced DevicesView with add/edit/remove and restart-pending state, backend validation of audio_output syntax, fragment directory management.

**Deferred for later:**
- Audio hardware discovery (listing available ALSA/PulseAudio devices from the server)
- CamillaDSP device profile management (already handled by DspView)
- Per-output volume control (MPD does not support this)

**Outside this product's identity:**
- Managing MPD's main config beyond the `include` annotation
- Network renderer devices (AirPlay, Spotify Connect, DLNA)
- Multi-room or grouped device setups
- Device profiles for non-MPD backends

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Config fragment directory under `data_dir`.** Fragments live in `<data_dir>/mpd-outputs.d/` rather than alongside `mpd_config`. This keeps managed state self-contained in Oxide's data directory, avoids scattering files across the filesystem, and survives a reinstall of MPD itself. The `include` directive bridges the two directories.
- KTD2. **One file per device named `<sanitized-name>.conf`.** Device names are sanitized (lowercased, non-alphanumeric → `-`, deduplicated `--`) so filenames are safe for MPD's glob include across Linux filesystems. One-file-per-device avoids merge conflicts and makes deletion trivial.
- KTD3. **Include directive injected before first `audio_output` block.** MPD processes `include` at the point it appears in the config; placing it before `audio_output` blocks ensures the fragment outputs are registered first and any duplicate-spec conflicts are resolved in favor of the fragment. The injection reads the file, inserts the line, and writes atomically (temp-file + rename) to match the config module's existing write strategy.
- KTD4. **User-triggered MPD restart, not auto-restart.** Restarting MPD drops active playback. A pending-restart state lets the admin finish configuring multiple devices before restarting once. (session-settled: user-approved — chosen over auto-restart: auto-restart disrupts active playback on every write.)
- KTD5. **New API namespace `/api/devices/configs`, not overloading existing `/api/devices`.** The existing `/api/devices` returns MPD's runtime output list (volatile, reflects what MPD reports right now). The new endpoints manage persistent config files (durable, survive restarts). Overloading the same path would confuse the difference between "what MPD currently has" and "what we want MPD to have." The restart endpoint is the bridge between them.

### High-Level Technical Design

```mermaid
flowchart TB
  subgraph Frontend
    DV[DevicesView]
    AF[Add/Edit forms]
    RB[Restart banner]
  end

  subgraph Backend API
    GET_D[GET /api/devices]
    CRUD[POST/GET/PUT/DELETE /api/devices/configs]
    RST[POST /api/devices/restart-mpd]
    EN_DIS[POST /api/devices/{id}/enable|disable]
  end

  subgraph Backend Core
    MPD[MPD module]
    CFG[Config fragment manager]
    INJECT[Include directive injector]
  end

  subgraph Disk
    MPD_CONF[mpd.conf]
    FRAG_DIR[data_dir/mpd-outputs.d/]
  end

  subgraph MPD Process
    MPD_D[MPD daemon]
  end

  DV --> GET_D
  DV --> CRUD
  RB --> RST
  GET_D --> MPD
  EN_DIS --> MPD
  CRUD --> CFG
  CFG --> FRAG_DIR
  INJECT --> MPD_CONF
  RST --> MPD_D
  MPD_D -.->|outputs/enableoutput/disableoutput| MPD
```

**Flow for adding a device:**
1. Admin fills form in DevicesView → `POST /api/devices/configs` with `{type, name, device, ...}`
2. Backend validates required fields and sanitizes device name → writes `<sanitized-name>.conf` to fragment directory
3. On first write ever, backend checks `mpd_config` for an `include` line; if missing, injects `include "..."` before first `audio_output` block
4. Response returns `{restart_pending: true}`
5. Frontend shows device in managed list with "restart pending" badge + global restart banner
6. Admin clicks "Restart MPD" → `POST /api/devices/restart-mpd` → backend runs `mpd --kill` then reconnects via MPD client's reconnect logic
7. Frontend polls `GET /api/devices` and `GET /api/devices/configs` until both reflect the new state

---

## Implementation Units

### U1. Backend: MPD config fragment manager

- **Goal:** Create a backend module that manages MPD `audio_output {}` config fragments on disk.
- **Requirements:** R1, R2, R3
- **Dependencies:** None
- **Files:**
  - `backend/src/devices/` (new directory)
  - `backend/src/devices/mod.rs` — module root
  - `backend/src/devices/config_fragment.rs` — fragment manager
  - `backend/src/devices/include_injector.rs` — include directive injection into mpd.conf
- **Approach:**
  1. New `devices` module under `backend/src/`. Register in `backend/src/main.rs` or `backend/src/lib.rs`.
  2. `ConfigFragmentManager` struct holding the fragment directory path (`data_dir/mpd-outputs.d/`). Exposes:
     - `list()` — read all `.conf` files, parse key fields (name, type, device) from each
     - `create(name, type, device, format, mixer_type, mixer_device, dop)` — validate, sanitize name, write atomically
     - `update(old_name, fields)` — delete old file, write new. Fields match the create signature (all optional, only provided ones are updated).
     - `delete(name)` — remove file
  3. Validation: `type` must be non-empty and match known MPD output types (alsa, pulse, fifo, httpd, etc.); `name` must be non-empty; `device` is optional but must not contain newlines or nulls; `format` if set must match MPD's `BITS:RATE:CHANNELS` pattern; `mixer_type` if set must be `hardware`, `software`, or `none`; `mixer_device` if set must not contain newlines or nulls.
  4. `IncludeInjector` struct holds the path to `mpd_config`. Exposes:
     - `ensure_include(fragment_dir)` — read mpd_config, scan for `include "*/mpd-outputs.d/*.conf"`, if absent insert `include "/abs/path/to/mpd-outputs.d/*.conf"` on a new line before the first `audio_output {` line (or at end of file if none found), write atomically.
  5. Device name sanitization: lowercased, `[^a-zA-Z0-9._-]` → `-`, collapse `--+`, trim leading/trailing `-`.
  6. Fragment file template: write as a minified `audio_output { ... }` block (one field per line, no indentation beyond readability).
- **Execution note:** Implement the fragment manager standalone and unit-testable before wiring into API routes.
- **Test scenarios:**
  - Creating a fragment writes a valid `.conf` file with the expected `audio_output {}` block
  - Creating a fragment with an empty name or type is rejected with a clear error
  - Creating a fragment with a name containing special chars sanitizes to a safe filename
  - Listing returns all fragment files in the directory
  - Listing an empty directory returns an empty list
  - Updating a fragment replaces the old file with the new content
  - Updating a fragment whose old file no longer exists returns an error
  - Deleting a fragment removes the file from disk
  - Deleting a non-existent fragment returns an error
  - Fragment with optional fields omitted still produces valid `audio_output {}` syntax
- **Verification:** `cargo test` passes. Manual: run unit tests for `ConfigFragmentManager` and `IncludeInjector`.

### U2. Backend: Include directive injection

- **Goal:** Auto-inject the `include` directive into MPD's config file so MPD loads managed fragments on restart.
- **Requirements:** R10, R11
- **Dependencies:** U1
- **Files:**
  - `backend/src/devices/include_injector.rs` — implementation (from U1)
  - `backend/src/devices/config_fragment.rs` — call `include_injector.ensure_include()` on first write
- **Approach:**
  1. `IncludeInjector::ensure_include(config_path, fragment_dir)`:
     - Read `config_path` as text.
     - Search for `include ` followed by a path containing `mpd-outputs.d` (to handle existing include lines).
     - If found and the path matches the current fragment dir → return Ok (no-op).
     - If found but the path is stale (different fragment dir) → replace the line.
     - If not found → insert `include "/abs/path/to/mpd-outputs.d/*.conf"` on a new line before the first line that starts with `audio_output {`. If no such line exists, append at end of file.
     - Write modified content atomically (temp file + rename).
  2. Guard: only run when `mpd_config` is `Some(path)` in Oxide's config.
  3. Permission handling: the atomic write (temp file + rename) falls through to a descriptive error message naming the MPD config path and the likely permission issue when writes fail. Systemd-managed MPD may require `mpd_config` to point to a writable config location.
  4. If `mpd_config` is `None`, store a flag so the API response includes `include_warning: true`.
- **Execution note:** Test the injector against a temp file to avoid modifying the real mpd.conf during testing.
- **Test scenarios:**
  - Injecting include into a config with no existing include adds the line before `audio_output {`
  - Injecting include into a config with no `audio_output {` at all appends at end of file
  - Injecting include when an include line already exists with the correct path is a no-op
  - Injecting include with a stale path replaces the stale path
  - Atomically written file survives a simulated crash (temp file exists, target is untouched)
  - Calling inject on a config with `mpd_config = None` returns a warning without panicking
- **Verification:** `cargo test` passes. Manual: run against a temp copy of an mpd.conf fixture.

### U3. Backend: Device config API routes

- **Goal:** Add REST endpoints for device config CRUD and MPD restart trigger.
- **Requirements:** R4, R5, R6, R7, R8, R9
- **Dependencies:** U1, U2
- **Files:**
  - `backend/src/api/mod.rs` — add new routes
  - `backend/src/devices/config_fragment.rs` — consumed by handlers
  - `backend/src/devices/mod.rs` — expose fragment manager via AppState
  - `backend/src/state.rs` — add fragment manager to AppState
- **Approach:**
  1. Register these routes in the Axum router in `api/mod.rs`:
     - `GET /api/devices/configs` — `list_device_configs`
     - `POST /api/devices/configs` — `create_device_config`
     - `PUT /api/devices/configs/{name}` — `update_device_config`
     - `DELETE /api/devices/configs/{name}` — `delete_device_config`
     - `POST /api/devices/restart-mpd` — `restart_mpd`
  2. `create_device_config` handler:
     - Deserialize body with `type`, `name`, `device`, `format`, `mixer_type`, `mixer_device`, `dop`
     - Call `config_fragment_manager.create(...)`
     - On first-ever create, call `include_injector.ensure_include(mpd_config, fragment_dir)`
     - Return `{name, restart_pending: true, include_warning: bool}`
  3. `restart_mpd` handler:
     - Check `is_localhost(host)` first; return a clear error if MPD is remote (cannot restart a non-local daemon)
     - Kill MPD via `mpd --kill` (or signal from the Mpd struct)
     - Call `mpd.ensure_running()` which reconnects
     - Return `{status: "ok"}` when reachable, or an error if MPD fails to start
  4. The existing `/api/devices` (GET) stays unchanged — it reports runtime state from MPD's `outputs` command.
  5. `AppState` gains a `ConfigFragmentManager` field (constructed at startup from `config.data_dir`).
- **Test scenarios:**
  - `POST /api/devices/configs` with valid body returns 201 and `{restart_pending: true}`
  - `POST /api/devices/configs` with missing `type` returns 422 with field-level validation error
  - `GET /api/devices/configs` returns array of managed configs with a `restart_pending: bool` field (true when any config has been created/updated/deleted since the last MPD restart)
  - `PUT /api/devices/configs/{name}` with valid body returns 200 and updates the fragment on disk
  - `PUT /api/devices/configs/{name}` for a non-existent name returns 404
  - `DELETE /api/devices/configs/{name}` removes the fragment file and returns 200
  - `DELETE /api/devices/configs/{name}` for non-existent name returns 404
  - `POST /api/devices/restart-mpd` returns 200 when MPD was running, restarts, and becomes reachable
  - Existing `GET /api/devices`, enable, and disable endpoints continue to work unchanged
- **Verification:** `cargo test` passes. Manual: start backend, call each endpoint with curl.

### U4. Frontend: Enhanced DevicesView

- **Goal:** Upgrade DevicesView with add/edit/remove workflows and a restart-pending banner.
- **Requirements:** R12, R13, R14, R15, R16, R17
- **Dependencies:** U3 (API exists)
- **Files:**
  - `frontend/src/components/DevicesView.tsx` — enhanced component
  - `frontend/src/components/DevicesView.module.css` — new styles for forms and banner
  - `frontend/src/components/DeviceConfigForm.tsx` — new form component (add/edit reuse)
  - `frontend/src/components/DeviceConfigForm.module.css` — form styles
  - `frontend/src/api.ts` — add new API methods
  - `frontend/src/types.ts` — add `DeviceConfig` type
- **Approach:**
  1. Add new types to `types.ts`:
     ```typescript
     interface DeviceConfig {
       name: string
       type: string
       device: string | null
       format: string | null
       mixer_type: string | null
       dop: boolean
     }
     interface DeviceConfigAction {
       restart_pending: boolean
       include_warning?: boolean
     }
     ```
  2. Add API methods to `api.ts`:
     - `deviceConfigs()` → `GET /api/devices/configs`
     - `createDeviceConfig(cfg)` → `POST /api/devices/configs`
     - `updateDeviceConfig(name, cfg)` → `PUT /api/devices/configs/{name}`
     - `deleteDeviceConfig(name)` → `DELETE /api/devices/configs/{name}`
     - `restartMpd()` → `POST /api/devices/restart-mpd`
  3. `DeviceConfigForm` component (reused for add and edit):
     - Fields: type (dropdown + custom), name (text), device (text), format (text), mixer_type (text, optional), dop (checkbox)
     - Validates required fields on submit
     - Emits `onSave(formData)` callback
  4. Enhanced `DevicesView`:
     - Two sections: "Runtime devices" (existing enable/disable list) and "Configured devices" (managed configs with edit/remove buttons)
     - "Add device" button toggles inline form (or modal)
     - Edit button populates form from existing config
     - Remove button shows confirmation before deleting
     - **Restart-pending banner** at the top: "Device configs changed — restart MPD to apply" with a "Restart MPD" button. Visible when any config action has been taken since last restart. Track via a local `restartPending` state initialized from the `restart_pending` field in the configs list response (true on create/update/delete, false after successful restart). On initial page load, `GET /api/devices/configs` provides the restart-pending state so the banner survives a browser refresh.
     - Progress indicator during restart ("Restarting MPD..." disabled button)
  5. CSS for new elements: form layout, restart banner (highlighted, action-oriented), confirmation dialog (simple confirm/ cancel or inline).
  6. The `ConfigView.tsx` card remains but uses the enhanced `DevicesView` — no routing change needed since DevicesView is already embedded.
- **Execution note:** Build the form component and restart banner as incremental extensions of the existing component tree. Do not introduce a modal library — use inline expand/collapse.
- **Test scenarios:**
  - DevicesView renders both runtime device list and managed config list
  - "Add device" form appears on button click and disappears on cancel
  - Submitting the add form with empty required name shows validation error
  - Submitting valid add form calls `POST /api/devices/configs` and sets restart pending
  - Edit form is pre-populated with existing config values
  - Remove button triggers confirmation dialog; confirming calls DELETE
  - Restart-pending banner appears after create/update/delete
  - Restart-pending banner disappears after successful restart
  - Restart button shows disabled state during restart with "Restarting…" text
- **Verification:** `npx vitest run` passes. Manual: start dev server, verify each flow in browser.

### U5. Backend: Config validation and error handling

- **Goal:** Ensure device config fragments produce valid MPD syntax and surface clear errors for malformed input.
- **Requirements:** R2 (implicit in validation)
- **Dependencies:** U1
- **Files:**
  - `backend/src/devices/config_fragment.rs` — validate method
  - `backend/src/error.rs` — add `DeviceConfig` error variant if needed
- **Approach:**
  1. Extend the fragment manager with a `validate_config(type, name, device, format, mixer_type, mixer_device, dop)` function:
     - `type`: must be non-empty, must match one of the known MPD output driver names (`alsa`, `pulse`, `fifo`, `httpd`, `shout`, `recorder`, `null`, `pipe`, `jack`, `opensl`, `osx`, `wasapi`, `winmm`) or be accepted as-is for forward compat
     - `name`: non-empty, no newlines, no nulls
     - `device`: optional, if present no newlines or nulls
     - `format`: optional, if present must match `/^\d+:\d+:\d+$/` (BITS:RATE:CHANNELS)
     - `mixer_type`: optional, if present must be `hardware`, `software`, or `none`
     - `mixer_device`: optional, if present no newlines or nulls
     - `dop`: boolean, no extra validation
  2. Return a structured `ValidationResult` with field-level errors, so the API can return a 422 with which field failed and why.
  3. Add a `DeviceConfigError` variant to `AppError` (or reuse `BadRequest`) for validation failures.
- **Test scenarios:**
  - Valid config passes validation
  - Empty type is rejected with field-level error
  - Unknown type passes through with warning (forward-compat)
  - Malformed format (`44100:foo:2`) is rejected
  - Device name with newline character is rejected
  - All optional fields omitted passes validation
- **Verification:** `cargo test` passes.

---

## Verification Contract

| Gate | Command | Unit |
|------|---------|------|
| Backend unit tests | `cargo test` | All units |
| Backend build | `cargo build` | All units |
| Frontend unit tests | `npm test` (from `frontend/`) | U4 |
| Frontend type check | `npx tsc --noEmit` (from `frontend/`) | U4 |
| End-to-end (manual) | Start backend + frontend, navigate to Settings > Playback devices, add/edit/remove a device, restart MPD | All units |

---

## Definition of Done

1. All implementation units are implemented and their test scenarios pass.
2. `cargo test` passes (zero new failures, ignoring pre-existing visuzalizer build errors).
3. `npm test` passes (frontend).
4. Manual smoke test: add a device via the UI → config fragment appears on disk → restart MPD → device appears in MPD output list.
5. Manual edge case: create a device with `mpd_config` unset → fragment is written on disk → include-warning notice is displayed.
6. Manual edge case: device CRUD without restarting MPD → restart-pending banner persists; restarting MPD clears it and device appears.
7. Existing device enable/disable toggles still work after changes.
