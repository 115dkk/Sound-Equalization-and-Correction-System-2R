# 0045. 작성기 공용 emit module을 secs-graph로

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A4. 해결하는 기술 부채 발견: TD-222(작성기 사이의 작은 복사본), TD-218의 원자적 교체 사본, TD-267(`pyfmt`의 비유한값), TD-224(머리 주석의 줄바꿈). 관련 ADR: 0004(골든), 0016(공개 크레이트), 0017(출력 레이트), 0021(설계 식별).

## 배경

`ExportGraph`(`crates/secs-graph`)는 지난 리뷰에서 토폴로지 검증과 파일 묶음을 모았지만, 작성기 셋이 공유하는 '내보낼 때 글자와 바이트를 만드는 규칙'은 모으지 못했습니다. 다시 센 복사본은 다음과 같습니다.

- 숫자 포매터 `pyfmt`(`crates/secs-camilla/src/pyfmt.rs`, 245줄: `round_ndigits`, `repr_f64`, `format_g`, `format_fixed`, `format_i3`, `format_s30`)를 `secs-filters/src/lib.rs:19`, `playback.rs:15`, `secs-export/src/lib.rs:11`이 가져다 씁니다. 그래서 `secs-filters`와 `secs-export`가 포매터 하나 때문에 `secs-camilla`를 의존합니다(`secs-export/Cargo.toml:20`, `secs-filters/Cargo.toml:21`). `docs/ARCHITECTURE.md` 3절 그림의 '숫자 포매터' 화살표가 그것입니다.
- Equalizer APO의 머리 주석 `eqapo_note`·`eqapo_note_with_bands`가 CamillaDSP 크레이트(`secs-camilla/src/note.rs:86-111`)에 있습니다.
- `finite` 세 벌(`secs-graph/src/validate.rs:12`, `secs-camilla/src/lib.rs:435`, `secs-filters/src/lib.rs:1919`), 세 규칙의 식별자 검사(`validate.rs:37`, `secs-filters:1972-1978`, `secs-camilla:421-433`), 같은 출력 이름 가드 두 벌(`secs-filters:1263-1274`, `secs-camilla:1529-1535`), 천 단위 구분 네 벌(`secs-engine/src/mimo/run.rs:171`, `secs-export:126`, `secs-filters:994-1003`, `secs-engine/src/notes.rs:202-211`).
- FIR을 출력 레이트 WAV로 펴는 방식이 두 가지 있습니다. `secs-filters:1234-1256`은 plan의 `FirView`를 읽고, `secs-camilla:814-818, 1104-1125`는 `MimoFilter`를 직접 읽습니다.
- 원자적 교체 네 벌(`secs-camilla:1492`, `secs-export:987`, `secs-project/src/record.rs:1416`, `secs-service/src/project_edit.rs:367`). `secs_types::fs::write_atomic_bytes`가 이미 있는데 넷이 남았습니다.

deletion test: 복사본 하나를 지우면 이웃 작성기에 똑같이 다시 나타나므로 지금은 모두 pass-through입니다.

## 결정

`crates/secs-graph/src/emit/`에 작성기 공용 module을 둡니다. 작성기 셋은 `secs-graph`만 읽습니다.

```text
secs_graph::emit
├── pyfmt       round_ndigits, repr_f64, format_g, format_fixed, format_i3, format_s30
│               (지금 secs-camilla의 것을 글자 그대로; nan·inf는 "nan"·"inf"로 먼저 돌려줍니다, TD-267)
├── note        mode_name, camilla_note, eqapo_note, eqapo_note_with_bands
├── guard       finite(name, value), identifier(name, value), distinct_outputs(names), no_line_breaks(name, value)
│               (오류는 GraphError::{NonFinite, Invalid}; 작성기는 From<GraphError>로 받습니다, TD-225)
├── grouped     grouped(n) -> String (천 단위 구분 하나)
└── fir_files   fir_files(plan: &FilterPlan, prefix: &str, rate: u32) -> Result<Vec<(String, Vec<u8>)>, GraphError>
                (plan.firs()의 FirView를 rate로 펴서 WAV 바이트까지; MimoFilter를 직접 읽는 두 번째 처리 방식을 지웁니다)
```

- `secs-filters`와 `secs-export`는 `secs-camilla` 의존을 지웁니다. `secs-camilla`는 `secs-graph`를 의존하므로 같은 함수를 `secs_graph::emit`에서 씁니다.
- 공개 크레이트 API(ADR 0016): `secs_camilla::pyfmt`는 한 판 동안 `#[deprecated] pub use secs_graph::emit::pyfmt::*`로 남기고 다음 minor에서 지웁니다. `secs_camilla::atomic_write`는 `secs_types::fs::write_atomic_bytes`로 같은 방식으로 넘어갑니다.
- 원자적 교체 네 벌은 `secs_types::fs::write_atomic_bytes`를 쓰고, 오류 종류가 필요한 서비스는 `ErrorCode`를 인자로 받는 도우미 하나(`secs-service/src/fs.rs`, A25의 결정)로 모읍니다.
- 48,000 상수 넷(`SAMPLE_RATE`, `DESIGN_RATE`, 재생 `SAMPLE_RATE: f64`, 내보내기 `SR`)은 `secs_dsp::rate::DESIGN_RATE` 하나를 가리키고, '모든 내보낸 FIR의 샘플레이트'라고 된 낡은 doc은 '설계 레이트'로 고칩니다(ADR 0017 이후의 뜻). 반 dB 헤드룸 올림 셋은 `secs_graph::headroom_db_half_db`, ties-to-even 구현 둘은 `f64::round_ties_even`, 서포트 묶기 둘은 `ExportGraph::support_groups_rounded`가 됩니다.

바뀌지 않는 것: 내보낸 파일의 바이트(골든이 고정), `make_config_observed`의 구조(ADR 0043의 결정대로 한 덩어리), 파일 이름 규칙(ADR 0021).

## 시험

`secs-graph`의 interface에서 먼저 씁니다.

- `emit::pyfmt`의 시험은 `secs-camilla/src/pyfmt.rs`의 시험 module을 그대로 옮기고, `nan`·`inf` 사례와 음의 영 사례를 추가합니다(기대 문자열은 CPython의 `repr`·`format` 출력 리터럴).
- `emit::guard`의 시험은 세 작성기에 흩어진 거절 사례(`finite`, 식별자, 같은 출력 이름)를 한 벌로 모으고, 줄바꿈 거절(`\n`, `\r`)을 추가합니다(TD-224).
- `emit::fir_files`의 시험은 오라클 `computed_dolby` 픽스처의 plan으로 48 kHz와 96 kHz에서 만든 WAV 바이트가 지금 `secs-camilla`와 `secs-filters`의 골든 파일과 같은지 봅니다.
- 골든 시험(`secs-camilla/tests/golden.rs`, `secs-filters/tests/golden.rs`, `secs-export`의 mdat 골든)은 한 줄도 바뀌지 않고 통과해야 합니다. 그것이 이 ADR의 가장 강한 시험입니다.
- 옮기는 시험: 세 작성기의 `finite`·식별자·이름 가드 시험, `secs-camilla`의 `pyfmt` 시험.

## 이행

1. `secs_graph::emit::pyfmt`를 만들고 시험을 옮깁니다. `secs-camilla::pyfmt`는 재수출로 둡니다. 커밋 하나.
2. `secs-filters`와 `secs-export`가 `secs_graph::emit::pyfmt`를 쓰게 하고 `secs-camilla` 의존을 지웁니다. `cargo deny`와 `cargo publish --dry-run`이 통과해야 합니다(의존 그림이 바뀝니다). 커밋 하나.
3. `emit::note`와 `emit::guard`, `emit::grouped`를 만들고 세 작성기와 `secs-engine/src/notes.rs`의 복사본을 지웁니다. 커밋 둘.
4. `emit::fir_files`를 만들고 `secs-camilla`의 `MimoFilter` 직접 읽기를 지웁니다. 골든 비트 동일. 커밋 하나.
5. 원자적 교체 넷과 48,000 상수 넷, 헤드룸·반올림·묶기 복사본을 정리합니다. 커밋 둘.
6. `docs/ARCHITECTURE.md` 3절 그림에서 `secs-camilla → secs-filters/secs-export` 화살표를 지웁니다. 커밋 하나.

작업 분배: Daybreak(작성기 크레이트). Claude가 커밋마다 디프를 검토하며, 검토가 보는 것은 옮긴 함수의 본문이 글자 그대로인지(포매터는 한 글자도 바뀌면 안 됩니다), 골든 시험 파일에 손대지 않았는지, `Cargo.toml`의 의존 변경이 둘뿐인지입니다.

## 결과

- 파일 이름 안전 규칙과 반올림 정책, FIR WAV 생성이 `secs-graph` 한 module에 있습니다. 작성기 셋 모두 `secs-graph`만 읽어 의존 그림이 단순해집니다.
- 출력 바이트는 바뀌지 않으므로 편차 항목은 없습니다.
- 위험: `secs-camilla`의 공개 API에서 `pyfmt`와 `atomic_write`가 빠지는 것은 라이브러리 판의 변경입니다. 한 판의 `#[deprecated]` 유예와 CHANGELOG 항목으로 알립니다.
- 다음 감사는 `secs-dsp`의 작은 module(`minphase`, `grid`, `peaks`, `lowrate`, `numeric`)을 합치라고 다시 제안하지 않아야 합니다. 각각 NumPy·SciPy의 규칙 하나를 담고 골든이 고정하므로 깊습니다. 이 ADR은 작성기의 글자·바이트 규칙만 모읍니다.
