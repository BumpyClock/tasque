# Changelog

## [0.8.0](https://github.com/BumpyClock/tasque/compare/v0.7.0...v0.8.0) (2026-07-02)


### Features

* **watch:** dedicated OpenTUI live view for tsq watch ([#4](https://github.com/BumpyClock/tasque/issues/4)) ([7b04b17](https://github.com/BumpyClock/tasque/commit/7b04b17f07c5916ddba6577cceef48be51ed85cc))


### Bug Fixes

* address CodeRabbit PR feedback ([84f69b4](https://github.com/BumpyClock/tasque/commit/84f69b40266fb7cee5207f27a7e9a0629d5cd50c))
* address follow-up PR comments ([1d731e8](https://github.com/BumpyClock/tasque/commit/1d731e85ee248a9ba65eae4305ad0ec1c11ad178))
* **render:** escape control sequences in human output ([8c5183f](https://github.com/BumpyClock/tasque/commit/8c5183f2591c7f0c0c8f55bcb4fce84e3c500067))
* repair OpenTUI CLI contract, sync-branch integrity, and output/skill-source hardening ([6066497](https://github.com/BumpyClock/tasque/commit/6066497cf805e5207381ef72e97cffca4ca56212))
* **skills:** drop CWD from implicit skill source roots ([3be168c](https://github.com/BumpyClock/tasque/commit/3be168c7d2624a62d2659c48a31c4de5555e708b))
* **sync:** configure merge driver when materializing sync worktree ([d716cc3](https://github.com/BumpyClock/tasque/commit/d716cc3c3cf83d1994943e2a72d186a0ea4c0944))
* **sync:** make implicit migration push best-effort and clear events after local commit ([720c100](https://github.com/BumpyClock/tasque/commit/720c1004f2b291843c2d26a2e0926546348fe0b7))
* **tui:** fetch tasks via watch --once and dep trees via deps verb ([2a29593](https://github.com/BumpyClock/tasque/commit/2a29593e29ee489aca9d93c10e43791e51269548))
* **tui:** load specs via tsq spec --show and list all epics ([881eb28](https://github.com/BumpyClock/tasque/commit/881eb28301a0a1d85249eef8464c5c2451f9fb0a))


### Performance Improvements

* **tui:** move tsq subprocess calls off the render thread ([15f2076](https://github.com/BumpyClock/tasque/commit/15f207628e714dcbef79f02554ddd39ef6a2cd0f))
