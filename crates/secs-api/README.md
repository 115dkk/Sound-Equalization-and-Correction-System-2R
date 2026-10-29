# SECS 2R Rust API

`sound-equalization-and-correction-system-rust-api` is the supported Rust API for SECS 2R. SECS 2R designs room correction from loudspeaker measurements and is a Rust and Tauri port of SECS 2.

## Features

- `engine` is enabled by default. It embeds the file-based project, calibration, MIMO, result, and export service without the desktop app.
- `client` talks to a running SECS 2R desktop app through its authenticated loopback HTTP and WebSocket API. Use `--no-default-features --features client` for a client-only build.

The public API does not include SECS 2R's native operating-system audio adapters or GPU acceleration. An `engine` user may inject an implementation of `secs_audio::AudioBackend`.

## Install

```sh
cargo add sound-equalization-and-correction-system-rust-api --rename secs
```

Enable only the running-app client with:

```sh
cargo add sound-equalization-and-correction-system-rust-api --rename secs --no-default-features --features client
```

## Embed the engine

```rust,no_run
use std::time::Duration;
use secs::types::ipc::{CalibrateParams, JobEvent, ProjectPathParams};
use secs::{Engine, Request, Response, Secs};

fn main() -> secs::Result<()> {
    let engine = Engine::new();
    engine.call(Request::ProjectOpen(ProjectPathParams {
        path: "room.secs".into(),
    }))?;

    let Response::Calibrate(started) =
        engine.call(Request::Calibrate(CalibrateParams::default()))?
    else {
        unreachable!()
    };
    let JobEvent::Finished { .. } =
        engine.wait_job(started.job, Duration::from_secs(120))?
    else {
        unreachable!()
    };

    let Response::Results(results) = engine.call(Request::Results)? else {
        unreachable!()
    };
    println!("{results:#?}");
    Ok(())
}
```

Calibration results can then be exported by calling `Request::Export` with `ExportKind::CamillaDsp` and waiting for the returned job in the same way.

## Control the running app

```rust,no_run
use secs::types::local_api::DEFAULT_PORT;
use secs::Client;

fn main() -> secs::Result<()> {
    let client = Client::from_token_file(DEFAULT_PORT, Client::default_token_path())?;
    println!("{:#?}", client.status()?);
    for event in client.events()? {
        println!("{:#?}", event?);
    }
    Ok(())
}
```

## Version policy

This public crate follows its own 0.x release line. A minor release in the 0.x line may contain incompatible public API changes. The internal `secs-*` crates are pinned to exactly the same library version and carry no compatibility promise; depend on this crate instead.

## Licence and repository

This crate is licensed under GPL-3.0-only. Programs that link it form a combined work and must use GPL-3.0-only-compatible distribution terms when conveyed to others.

Repository: <https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R>

## 한국어

SECS 2R은 스피커 측정값으로 방 보정 필터를 설계합니다. SECS 2를 Rust와 Tauri로 옮긴 프로그램입니다. 기본 `engine` 기능은 앱 없이 프로젝트를 열고 보정과 내보내기를 실행하며, `client` 기능은 실행 중인 앱의 로컬 API를 부릅니다.

```sh
cargo add sound-equalization-and-correction-system-rust-api --rename secs
```

```rust,no_run
use secs::{Engine, Request, Response, Secs};

fn main() -> secs::Result<()> {
    let engine = Engine::new();
    let Response::Bootstrap(info) = engine.call(Request::Bootstrap)? else {
        unreachable!()
    };
    println!("SECS 2R {}", info.version);
    Ok(())
}
```

이 공개 크레이트는 별도의 0.x 판을 따릅니다. 내부 `secs-*` 크레이트의 호환성은 보장하지 않습니다. 라이선스는 GPL-3.0-only이며 저장소는 <https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R>입니다.

### 도움 주신 분들

SECS는 한국 오디오 커뮤니티 스피커 갤러리의 Sound Equalization and Correction System 프로젝트에서 시작했습니다. 그 뒤를 이은 [SECS 2](https://github.com/Kim-O-O/Sound-Equalization-and-Correction-System-2)가 소스를 GPL v3로 공개한 덕분에 SECS 2R과 이 크레이트가 있습니다. 보정 계산은 SECS 2의 방법을 그대로 따릅니다. SECS와 SECS 2를 만든 스피커 갤러리 여러분과 업스트림 저작자께 감사드립니다.
