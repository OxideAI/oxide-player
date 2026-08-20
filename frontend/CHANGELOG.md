# Changelog

## [0.2.0](https://github.com/OxideAI/oxide-player/compare/oxide-player-frontend-v0.1.0...oxide-player-frontend-v0.2.0) (2026-07-30)

### Features

- add Bluetooth audio support (output + input) ([#86](https://github.com/OxideAI/oxide-player/issues/86)) ([7435689](https://github.com/OxideAI/oxide-player/commit/7435689fb3cd46ea697a6ca5acde40226afe20ea))
- add clear-queue button that keeps the current song playing ([#44](https://github.com/OxideAI/oxide-player/issues/44)) ([f97f28c](https://github.com/OxideAI/oxide-player/commit/f97f28c1c9ae174a781daf2c61acb7e2754fda1e))
- add MPD output device config management via config fragments ([406056f](https://github.com/OxideAI/oxide-player/commit/406056f3396fb3ae1f7d914b08f3f5d74176dbed))
- add MPD output device config management via config fragments ([#77](https://github.com/OxideAI/oxide-player/issues/77)) ([fb9d31e](https://github.com/OxideAI/oxide-player/commit/fb9d31ee0e63c35f659cd8900224f58f0668e85f))
- config view, DSP settings, playlist management + playback URI fix ([#19](https://github.com/OxideAI/oxide-player/issues/19)) ([42d785b](https://github.com/OxideAI/oxide-player/commit/42d785b05e79f89fd79652be8895eb9663042722))
- keyboard shortcuts + search view (issue [#20](https://github.com/OxideAI/oxide-player/issues/20)) ([#61](https://github.com/OxideAI/oxide-player/issues/61)) ([fee8f43](https://github.com/OxideAI/oxide-player/commit/fee8f4360db221bb823298ba440abf84ea4ae685))
- make Artist — Album subtitle clickable to open album view ([#28](https://github.com/OxideAI/oxide-player/issues/28)) ([#59](https://github.com/OxideAI/oxide-player/issues/59)) ([09f26ff](https://github.com/OxideAI/oxide-player/commit/09f26ff753601ef7671f33c78a4ead4903e6edf3))
- make the web app a Progressive Web App (issue [#4](https://github.com/OxideAI/oxide-player/issues/4)) ([#48](https://github.com/OxideAI/oxide-player/issues/48)) ([4b1f7f4](https://github.com/OxideAI/oxide-player/commit/4b1f7f4b540298c470b43593460932ae89c9675a))
- real-time FFT audio visualizer for Kiosk mode (issue [#6](https://github.com/OxideAI/oxide-player/issues/6)) ([#64](https://github.com/OxideAI/oxide-player/issues/64)) ([a323d82](https://github.com/OxideAI/oxide-player/commit/a323d8245c5b014807297054751ffd8e01dbdb58))
- real-time FFT audio visualizer for Kiosk mode (issue [#6](https://github.com/OxideAI/oxide-player/issues/6)) ([#65](https://github.com/OxideAI/oxide-player/issues/65)) ([1c5ee80](https://github.com/OxideAI/oxide-player/commit/1c5ee80cde316e0a8c989d6f13b080a0995e589f))
- redesign frontend UI with ethereal glass theme ([#9](https://github.com/OxideAI/oxide-player/issues/9)) ([fa7c3dc](https://github.com/OxideAI/oxide-player/commit/fa7c3dcbef4028f3586c35ed559f53f29b80bd60))
- replace localStorage state with URL-based routing ([#41](https://github.com/OxideAI/oxide-player/issues/41)) ([a8dd1c3](https://github.com/OxideAI/oxide-player/commit/a8dd1c3f3135883c205b365f99ba9296b28a76b8))
- show backend and frontend versions on Settings (issue [#53](https://github.com/OxideAI/oxide-player/issues/53)) ([#62](https://github.com/OxideAI/oxide-player/issues/62)) ([8d32ba7](https://github.com/OxideAI/oxide-player/commit/8d32ba7f6e55fa456d57ae0f3f45c4efe2b822b4))
- song position ([#42](https://github.com/OxideAI/oxide-player/issues/42)) ([ed5d2e4](https://github.com/OxideAI/oxide-player/commit/ed5d2e426cafbce1a3f42b9cba822cb0d4f3aed7))

### Bug Fixes

- animate now-playing indicators and fix broken keyframes ([#37](https://github.com/OxideAI/oxide-player/issues/37)) ([#45](https://github.com/OxideAI/oxide-player/issues/45)) ([fb7a41f](https://github.com/OxideAI/oxide-player/commit/fb7a41fa6fd94689088b8667dca96a1cbbd689d7))
- animate now-playing indicators and fix broken keyframes ([#37](https://github.com/OxideAI/oxide-player/issues/37)) ([#74](https://github.com/OxideAI/oxide-player/issues/74)) ([c57e5e5](https://github.com/OxideAI/oxide-player/commit/c57e5e54586325d08ac1d414be87b3cfbec04d60))
- apply DSP config to CamillaDSP + playlist view/edit/play ([#18](https://github.com/OxideAI/oxide-player/issues/18)) ([2e41611](https://github.com/OxideAI/oxide-player/commit/2e4161125b682acf2f8319524414624e35c9ab7b))
- eliminate progress artifacts, seek reversion, and Nothing playing flash ([#29](https://github.com/OxideAI/oxide-player/issues/29)) ([78eeb28](https://github.com/OxideAI/oxide-player/commit/78eeb289b5514db0b5a7ac8d9abd59b09f172e42))
- hide volume slider when MPD mixer is disabled ([#83](https://github.com/OxideAI/oxide-player/issues/83)) ([3b6884c](https://github.com/OxideAI/oxide-player/commit/3b6884cb6510ce65d6fd4d0b7e9716eab1056f02))
- key cover art by album instead of per track ([#31](https://github.com/OxideAI/oxide-player/issues/31)) ([#38](https://github.com/OxideAI/oxide-player/issues/38)) ([be81524](https://github.com/OxideAI/oxide-player/commit/be81524f5a2d2ee5da655e361447a621943e8ba8))
- persist config across restarts and view tab on page refresh ([#30](https://github.com/OxideAI/oxide-player/issues/30)) ([2102523](https://github.com/OxideAI/oxide-player/commit/21025232cef08e45dd454b25d24f35ca9fb5eadd))
- play library track directly instead of queueing it ([#32](https://github.com/OxideAI/oxide-player/issues/32)) ([#54](https://github.com/OxideAI/oxide-player/issues/54)) ([a51528f](https://github.com/OxideAI/oxide-player/commit/a51528f409574070e2506b05c278f0e1f6cb991f))
- PR feedback — partial updates, restart error handling, field name mapping ([8a8d6bf](https://github.com/OxideAI/oxide-player/commit/8a8d6bfb4c374d138944ff632df919b1ac6b4509))
- remove source albums, dedupe parent/child sources, track source in album metadata ([#46](https://github.com/OxideAI/oxide-player/issues/46)) ([#47](https://github.com/OxideAI/oxide-player/issues/47)) ([0715a4d](https://github.com/OxideAI/oxide-player/commit/0715a4d79615c54422dba81f6ed1bea67bb0b951))
- render FileInfo modal in a portal so it is not clipped by album-row transform ([#55](https://github.com/OxideAI/oxide-player/issues/55)) ([03fa48c](https://github.com/OxideAI/oxide-player/commit/03fa48cd2e3b8b36268d33782527567294dce516)), closes [#49](https://github.com/OxideAI/oxide-player/issues/49)
- resume play/next/prev from stopped state after restart ([#40](https://github.com/OxideAI/oxide-player/issues/40)) ([ffa472c](https://github.com/OxideAI/oxide-player/commit/ffa472c5f2feb4da8a98f1085c3b8d9c4864e752)), closes [#36](https://github.com/OxideAI/oxide-player/issues/36)
- smooth sliders, DSP validation, and decoupled parametric EQ ([#25](https://github.com/OxideAI/oxide-player/issues/25)) ([83e4ccd](https://github.com/OxideAI/oxide-player/commit/83e4ccd4bb6b246fa03e86a5bfe5055702d350cb))
