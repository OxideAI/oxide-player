# Changelog

## [0.11.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.10.0...oxide-player-v0.11.0) (2026-07-31)


### Features

* **bluetooth:** add wake & connect for sleeping speakers ([d5dbf4c](https://github.com/OxideAI/oxide-player/commit/d5dbf4c40ab49e194484ca8f3e629e11b8b4e92b))


### Bug Fixes

* surface unreadable library dirs + add install --fix-perms ([dac2a36](https://github.com/OxideAI/oxide-player/commit/dac2a36e1b7190d615e3803152057588c9ab0b41))

## [0.10.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.9.0...oxide-player-v0.10.0) (2026-07-31)


### Features

* **bluetooth:** add full Bluetooth audio support ([c675eeb](https://github.com/OxideAI/oxide-player/commit/c675eebb7b2390ff4965b72ca1d680e10976b49e))
* **bluetooth:** add full Bluetooth audio support ([#90](https://github.com/OxideAI/oxide-player/issues/90)) ([61d6d3c](https://github.com/OxideAI/oxide-player/commit/61d6d3c4120f0d377dd1fb491b32d4d10205b530))

## [0.6.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.5.1...oxide-player-v0.6.0) (2026-07-30)


### Features

* add Bluetooth audio support (output + input) ([#86](https://github.com/OxideAI/oxide-player/issues/86)) ([7435689](https://github.com/OxideAI/oxide-player/commit/7435689fb3cd46ea697a6ca5acde40226afe20ea))

## [0.5.1](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.5.0...oxide-player-v0.5.1) (2026-07-30)


### Bug Fixes

* hide volume slider when MPD mixer is disabled ([#83](https://github.com/OxideAI/oxide-player/issues/83)) ([3b6884c](https://github.com/OxideAI/oxide-player/commit/3b6884cb6510ce65d6fd4d0b7e9716eab1056f02))

## [0.5.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.4.0...oxide-player-v0.5.0) (2026-07-30)


### Features

* add default sound ([a2d0099](https://github.com/OxideAI/oxide-player/commit/a2d009942b5059186801136d96b33e7615723ed2))

## [0.4.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.3.2...oxide-player-v0.4.0) (2026-07-29)


### Features

* add MPD output device config management via config fragments ([#77](https://github.com/OxideAI/oxide-player/issues/77)) ([fb9d31e](https://github.com/OxideAI/oxide-player/commit/fb9d31ee0e63c35f659cd8900224f58f0668e85f))

## [0.3.2](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.3.1...oxide-player-v0.3.2) (2026-07-29)


### Bug Fixes

* animate now-playing indicators and fix broken keyframes ([#37](https://github.com/OxideAI/oxide-player/issues/37)) ([#74](https://github.com/OxideAI/oxide-player/issues/74)) ([c57e5e5](https://github.com/OxideAI/oxide-player/commit/c57e5e54586325d08ac1d414be87b3cfbec04d60))
* handle non-UTF-8 filenames in scanner and warn when mpd_music_directory is unset ([9519e8c](https://github.com/OxideAI/oxide-player/commit/9519e8c89987d11a652b3a463c8d31bbed2e8375))

## [0.3.1](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.3.0...oxide-player-v0.3.1) (2026-07-29)


### Bug Fixes

* extract listen_addr before config is moved into AppState ([7b2ef6d](https://github.com/OxideAI/oxide-player/commit/7b2ef6d531e314e1a8490dd25ab8cf84094d5816))
* use config.listen instead of cli.listen for TCP bind ([fa1c704](https://github.com/OxideAI/oxide-player/commit/fa1c70460c85e828eb44e24e25800766025130a2))

## [0.3.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.2.0...oxide-player-v0.3.0) (2026-07-18)


### Features

* add clear-queue button that keeps the current song playing ([#44](https://github.com/OxideAI/oxide-player/issues/44)) ([f97f28c](https://github.com/OxideAI/oxide-player/commit/f97f28c1c9ae174a781daf2c61acb7e2754fda1e))
* config view, DSP settings, playlist management + playback URI fix ([#19](https://github.com/OxideAI/oxide-player/issues/19)) ([42d785b](https://github.com/OxideAI/oxide-player/commit/42d785b05e79f89fd79652be8895eb9663042722))
* graceful shutdown stops MPD playback on exit ([3c2e05c](https://github.com/OxideAI/oxide-player/commit/3c2e05c0e640fadf210ade1b26c86c380f2b02a7))
* make the web app a Progressive Web App (issue [#4](https://github.com/OxideAI/oxide-player/issues/4)) ([#48](https://github.com/OxideAI/oxide-player/issues/48)) ([4b1f7f4](https://github.com/OxideAI/oxide-player/commit/4b1f7f4b540298c470b43593460932ae89c9675a))
* optimize cover art on scan (downscale + recompress, issue [#39](https://github.com/OxideAI/oxide-player/issues/39)) ([#60](https://github.com/OxideAI/oxide-player/issues/60)) ([2a42549](https://github.com/OxideAI/oxide-player/commit/2a425494aa3faeff32554c2d0b0c070663ab526e))
* real-time FFT audio visualizer for Kiosk mode (issue [#6](https://github.com/OxideAI/oxide-player/issues/6)) ([#64](https://github.com/OxideAI/oxide-player/issues/64)) ([a323d82](https://github.com/OxideAI/oxide-player/commit/a323d8245c5b014807297054751ffd8e01dbdb58))
* real-time FFT audio visualizer for Kiosk mode (issue [#6](https://github.com/OxideAI/oxide-player/issues/6)) ([#65](https://github.com/OxideAI/oxide-player/issues/65)) ([1c5ee80](https://github.com/OxideAI/oxide-player/commit/1c5ee80cde316e0a8c989d6f13b080a0995e589f))
* release workflow that builds prebuilt deployment packages (issue [#52](https://github.com/OxideAI/oxide-player/issues/52)) ([#63](https://github.com/OxideAI/oxide-player/issues/63)) ([4277686](https://github.com/OxideAI/oxide-player/commit/42776866111deb6224d0c67b4aaf577cc718e4e1))
* show backend and frontend versions on Settings (issue [#53](https://github.com/OxideAI/oxide-player/issues/53)) ([#62](https://github.com/OxideAI/oxide-player/issues/62)) ([8d32ba7](https://github.com/OxideAI/oxide-player/commit/8d32ba7f6e55fa456d57ae0f3f45c4efe2b822b4))
* song position ([#42](https://github.com/OxideAI/oxide-player/issues/42)) ([ed5d2e4](https://github.com/OxideAI/oxide-player/commit/ed5d2e426cafbce1a3f42b9cba822cb0d4f3aed7))


### Bug Fixes

* apply DSP config to CamillaDSP + playlist view/edit/play ([#18](https://github.com/OxideAI/oxide-player/issues/18)) ([2e41611](https://github.com/OxideAI/oxide-player/commit/2e4161125b682acf2f8319524414624e35c9ab7b))
* eliminate progress artifacts, seek reversion, and Nothing playing flash ([#29](https://github.com/OxideAI/oxide-player/issues/29)) ([78eeb28](https://github.com/OxideAI/oxide-player/commit/78eeb289b5514db0b5a7ac8d9abd59b09f172e42))
* key cover art by album instead of per track ([#31](https://github.com/OxideAI/oxide-player/issues/31)) ([#38](https://github.com/OxideAI/oxide-player/issues/38)) ([be81524](https://github.com/OxideAI/oxide-player/commit/be81524f5a2d2ee5da655e361447a621943e8ba8))
* persist config across restarts and view tab on page refresh ([#30](https://github.com/OxideAI/oxide-player/issues/30)) ([2102523](https://github.com/OxideAI/oxide-player/commit/21025232cef08e45dd454b25d24f35ca9fb5eadd))
* play library track directly instead of queueing it ([#32](https://github.com/OxideAI/oxide-player/issues/32)) ([#54](https://github.com/OxideAI/oxide-player/issues/54)) ([a51528f](https://github.com/OxideAI/oxide-player/commit/a51528f409574070e2506b05c278f0e1f6cb991f))
* remove source albums, dedupe parent/child sources, track source in album metadata ([#46](https://github.com/OxideAI/oxide-player/issues/46)) ([#47](https://github.com/OxideAI/oxide-player/issues/47)) ([0715a4d](https://github.com/OxideAI/oxide-player/commit/0715a4d79615c54422dba81f6ed1bea67bb0b951))
* resolve CUE now-playing after restart ([#43](https://github.com/OxideAI/oxide-player/issues/43)) ([cbbe560](https://github.com/OxideAI/oxide-player/commit/cbbe5605ef230788a5301bb0a7e9775b48b69e83))
* resume play/next/prev from stopped state after restart ([#40](https://github.com/OxideAI/oxide-player/issues/40)) ([ffa472c](https://github.com/OxideAI/oxide-player/commit/ffa472c5f2feb4da8a98f1085c3b8d9c4864e752)), closes [#36](https://github.com/OxideAI/oxide-player/issues/36)
* scanner respects .mpdignore patterns in walk and prune ([#35](https://github.com/OxideAI/oxide-player/issues/35)) ([f27dd5c](https://github.com/OxideAI/oxide-player/commit/f27dd5c9c8081917fabfa4bf63d727e3d050544c))
* smooth sliders, DSP validation, and decoupled parametric EQ ([#25](https://github.com/OxideAI/oxide-player/issues/25)) ([83e4ccd](https://github.com/OxideAI/oxide-player/commit/83e4ccd4bb6b246fa03e86a5bfe5055702d350cb))

## [0.2.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-v0.1.0...oxide-player-v0.2.0) (2026-07-18)


### Features

* add clear-queue button that keeps the current song playing ([#44](https://github.com/OxideAI/oxide-player/issues/44)) ([f97f28c](https://github.com/OxideAI/oxide-player/commit/f97f28c1c9ae174a781daf2c61acb7e2754fda1e))
* config view, DSP settings, playlist management + playback URI fix ([#19](https://github.com/OxideAI/oxide-player/issues/19)) ([42d785b](https://github.com/OxideAI/oxide-player/commit/42d785b05e79f89fd79652be8895eb9663042722))
* graceful shutdown stops MPD playback on exit ([3c2e05c](https://github.com/OxideAI/oxide-player/commit/3c2e05c0e640fadf210ade1b26c86c380f2b02a7))
* make the web app a Progressive Web App (issue [#4](https://github.com/OxideAI/oxide-player/issues/4)) ([#48](https://github.com/OxideAI/oxide-player/issues/48)) ([4b1f7f4](https://github.com/OxideAI/oxide-player/commit/4b1f7f4b540298c470b43593460932ae89c9675a))
* optimize cover art on scan (downscale + recompress, issue [#39](https://github.com/OxideAI/oxide-player/issues/39)) ([#60](https://github.com/OxideAI/oxide-player/issues/60)) ([2a42549](https://github.com/OxideAI/oxide-player/commit/2a425494aa3faeff32554c2d0b0c070663ab526e))
* real-time FFT audio visualizer for Kiosk mode (issue [#6](https://github.com/OxideAI/oxide-player/issues/6)) ([#64](https://github.com/OxideAI/oxide-player/issues/64)) ([a323d82](https://github.com/OxideAI/oxide-player/commit/a323d8245c5b014807297054751ffd8e01dbdb58))
* real-time FFT audio visualizer for Kiosk mode (issue [#6](https://github.com/OxideAI/oxide-player/issues/6)) ([#65](https://github.com/OxideAI/oxide-player/issues/65)) ([1c5ee80](https://github.com/OxideAI/oxide-player/commit/1c5ee80cde316e0a8c989d6f13b080a0995e589f))
* release workflow that builds prebuilt deployment packages (issue [#52](https://github.com/OxideAI/oxide-player/issues/52)) ([#63](https://github.com/OxideAI/oxide-player/issues/63)) ([4277686](https://github.com/OxideAI/oxide-player/commit/42776866111deb6224d0c67b4aaf577cc718e4e1))
* show backend and frontend versions on Settings (issue [#53](https://github.com/OxideAI/oxide-player/issues/53)) ([#62](https://github.com/OxideAI/oxide-player/issues/62)) ([8d32ba7](https://github.com/OxideAI/oxide-player/commit/8d32ba7f6e55fa456d57ae0f3f45c4efe2b822b4))
* song position ([#42](https://github.com/OxideAI/oxide-player/issues/42)) ([ed5d2e4](https://github.com/OxideAI/oxide-player/commit/ed5d2e426cafbce1a3f42b9cba822cb0d4f3aed7))


### Bug Fixes

* apply DSP config to CamillaDSP + playlist view/edit/play ([#18](https://github.com/OxideAI/oxide-player/issues/18)) ([2e41611](https://github.com/OxideAI/oxide-player/commit/2e4161125b682acf2f8319524414624e35c9ab7b))
* eliminate progress artifacts, seek reversion, and Nothing playing flash ([#29](https://github.com/OxideAI/oxide-player/issues/29)) ([78eeb28](https://github.com/OxideAI/oxide-player/commit/78eeb289b5514db0b5a7ac8d9abd59b09f172e42))
* key cover art by album instead of per track ([#31](https://github.com/OxideAI/oxide-player/issues/31)) ([#38](https://github.com/OxideAI/oxide-player/issues/38)) ([be81524](https://github.com/OxideAI/oxide-player/commit/be81524f5a2d2ee5da655e361447a621943e8ba8))
* persist config across restarts and view tab on page refresh ([#30](https://github.com/OxideAI/oxide-player/issues/30)) ([2102523](https://github.com/OxideAI/oxide-player/commit/21025232cef08e45dd454b25d24f35ca9fb5eadd))
* play library track directly instead of queueing it ([#32](https://github.com/OxideAI/oxide-player/issues/32)) ([#54](https://github.com/OxideAI/oxide-player/issues/54)) ([a51528f](https://github.com/OxideAI/oxide-player/commit/a51528f409574070e2506b05c278f0e1f6cb991f))
* remove source albums, dedupe parent/child sources, track source in album metadata ([#46](https://github.com/OxideAI/oxide-player/issues/46)) ([#47](https://github.com/OxideAI/oxide-player/issues/47)) ([0715a4d](https://github.com/OxideAI/oxide-player/commit/0715a4d79615c54422dba81f6ed1bea67bb0b951))
* resolve CUE now-playing after restart ([#43](https://github.com/OxideAI/oxide-player/issues/43)) ([cbbe560](https://github.com/OxideAI/oxide-player/commit/cbbe5605ef230788a5301bb0a7e9775b48b69e83))
* resume play/next/prev from stopped state after restart ([#40](https://github.com/OxideAI/oxide-player/issues/40)) ([ffa472c](https://github.com/OxideAI/oxide-player/commit/ffa472c5f2feb4da8a98f1085c3b8d9c4864e752)), closes [#36](https://github.com/OxideAI/oxide-player/issues/36)
* scanner respects .mpdignore patterns in walk and prune ([#35](https://github.com/OxideAI/oxide-player/issues/35)) ([f27dd5c](https://github.com/OxideAI/oxide-player/commit/f27dd5c9c8081917fabfa4bf63d727e3d050544c))
* smooth sliders, DSP validation, and decoupled parametric EQ ([#25](https://github.com/OxideAI/oxide-player/issues/25)) ([83e4ccd](https://github.com/OxideAI/oxide-player/commit/83e4ccd4bb6b246fa03e86a5bfe5055702d350cb))
