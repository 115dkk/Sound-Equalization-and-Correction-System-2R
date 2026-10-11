"""P1 A1 `secs.model` 계약 검사 (트랙 E).

`P1 인터페이스 계약 2026-09-16.md` 의 "A1: `secs.model`" 절이 기준이다.
구현 전에 먼저 쓰인 테스트라 A1 이 `src/secs/model.py` 를 올리기 전에는
임포트 단계에서 실패한다(그게 정상이다. `importorskip` 으로 덮지 않는다).

기대값의 출처는 목업 `UI 목업/secs-ui-mockup.html` 이다.
- PRESETS            (304행)
- CHINFO             (312행)  역할 → [표시이름, 종류('main'|'sub'|'top'|'height'), 그룹] (10-10 탑 · 하이트 분리)
- defaultBands       (323행)
- ROLE_ORDER/SUB_ROLES (336~337행)
- setSubCount        (338행)
- DSP_PROFILES       (331행)
- targetAt           (370행)
- setTargetPreset    (373행)
"""
from __future__ import annotations

import json
import math

import numpy as np
import pytest

from conftest import engine_choice, load_engine

pytestmark = pytest.mark.skipif(
    engine_choice() != "package",
    reason="secs.model 은 패키지 엔진 전용이다 (SECS_ENGINE=package).",
)


@pytest.fixture(scope="module")
def model():
    """`secs.model` 모듈. A1 구현 전에는 ImportError 로 실패하는 게 정상."""
    load_engine("package")          # src 를 sys.path 에 올린다
    import secs.model as module     # type: ignore[import-not-found]
    return module


# ── 목업에서 그대로 옮긴 기대값 ────────────────────────────────────────

# 목업 PRESETS (304행)
MOCKUP_PRESETS: dict[str, tuple[str, ...]] = {
    "2.0": ("L", "R"),
    "2.1": ("L", "R", "SW1"),
    "2.2": ("L", "R", "SW1", "SW2"),
    "5.1": ("L", "R", "C", "SW1", "Ls", "Rs"),
    "7.1.4": ("L", "R", "C", "SW1", "Ls", "Rs", "Lb", "Rb",
              "Tfl", "Tfr", "Tbl", "Tbr"),
    "9.1.6": ("L", "R", "C", "SW1", "Ls", "Rs", "Lb", "Rb", "Lw", "Rw",
              "Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr"),
}

# 목업 ROLE_ORDER (336행) + 하이트 넷(10-10 사용자: 탑과 하이트는 다른 채널 — 목업에는 아직 없는 역할이라 끝에 덧붙임)
MOCKUP_ROLE_ORDER: tuple[str, ...] = (
    "L", "R", "C", "SW1", "SW2", "SW3", "SW4", "SW5", "SW6", "SW7", "SW8",
    "Ls", "Rs", "Lb", "Rb", "Lw", "Rw", "Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr",
    "Hfl", "Hfr", "Hbl", "Hbr",
)
MOCKUP_SUB_ROLES: tuple[str, ...] = tuple(f"SW{i}" for i in range(1, 9))

# 목업 CHINFO 의 종류(두 번째 원소)
MOCKUP_KIND: dict[str, str] = {
    "L": "main", "R": "main", "C": "main",
    **{f"SW{i}": "sub" for i in range(1, 9)},
    "Ls": "main", "Rs": "main", "Lb": "main", "Rb": "main",
    "Lw": "main", "Rw": "main",
    # 10-10: 목업은 T* 를 'height' 로 묶었지만 천장 탑은 'top', 벽 위쪽 하이트(H*)만 'height'(사용자: 다른 채널)
    "Tfl": "top", "Tfr": "top", "Tml": "top",
    "Tmr": "top", "Tbl": "top", "Tbr": "top",
    "Hfl": "height", "Hfr": "height", "Hbl": "height", "Hbr": "height",
}

# 목업 CHINFO 의 그룹(세 번째 원소) 이 만드는 분할.
# 결정 1 (리드, 2026-09-16): role_group 은 목업 한국어 그룹명을 그대로 쓰고
# 서브만 'sub' 이다. 그룹 이름 문자열 대신 분할(같은 그룹인가)로 검사하는
# 지금 방식을 유지한다. 종류('main'|'sub'|'height') 는 별도 role_kind().
MOCKUP_GROUP_PARTITION: tuple[tuple[str, ...], ...] = (
    ("L", "R"),                                              # 프론트
    ("C",),                                                  # 센터
    MOCKUP_SUB_ROLES,                                        # 서브
    ("Ls", "Rs"),                                            # 서라운드
    ("Lb", "Rb"),                                            # 백
    ("Lw", "Rw"),                                            # 와이드
    ("Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr"),              # 탑(10-10 까지 '하이트')
    ("Hfl", "Hfr", "Hbl", "Hbr"),                            # 하이트(10-10 새로)
)

# 목업 DSP_PROFILES (331행): 키 → (표시이름, firTaps, firBudget, iirBands)
MOCKUP_DSP_PROFILES: dict[str, tuple[str, int, int, int]] = {
    # 10-09 사용자: 실측 연구값(테시라 SERVER · Q-SYS 8 Flex/110f) · miniDSP 채널당 2,048 · 합계 4,096, BruteFIR 표기 뺌
    "tesira": ("Biamp Tesira SERVER (DSP-2 ×3)", 2048, 51200, 16),
    "minidsp": ("miniDSP Flex / 2x4 HD", 2048, 4096, 10),
    "qsys": ("Q-SYS Core 8 Flex / 110f", 16384, 65536, 24),
    "camilla": ("PC · CamillaDSP", 65536, 65536 * 16, 64),
    "custom": ("사용자 정의", 2048, 32768, 16),
}

# 목업 setTargetPreset (373행)
MOCKUP_TARGET_PRESETS: dict[str, tuple[tuple[float, float], ...]] = {
    "flat": ((25, 0), (200, 0), (1000, 0), (16000, 0)),
    "harman": ((25, 6), (60, 5), (120, 1), (1000, 0), (16000, -3)),
    "cinema": ((25, 4), (80, 3), (200, 0), (1000, -1), (16000, -5)),
}


# ── 상수 ───────────────────────────────────────────────────────────────

def test_roles_match_mockup_order(model):
    assert tuple(model.ROLES) == MOCKUP_ROLE_ORDER
    assert tuple(model.SUB_ROLES) == MOCKUP_SUB_ROLES
    assert tuple(model.BANDS) == ("FULL", "LF", "MF", "HF", "SUB")


def test_role_group_matches_mockup_partition(model):
    # 서브는 계약이 'sub' 로 못 박았다.
    for role in MOCKUP_SUB_ROLES:
        assert model.role_group(role) == "sub", role

    # 나머지는 이름이 아니라 분할(같은 그룹끼리 묶이는지)로 검사한다.
    seen: dict[str, str] = {}
    for members in MOCKUP_GROUP_PARTITION:
        groups = {model.role_group(role) for role in members}
        assert len(groups) == 1, f"{members} 가 한 그룹이 아니다: {groups}"
        name = groups.pop()
        assert name not in seen, f"그룹 이름 충돌: {members} 와 {seen[name]} 가 같은 '{name}'"
        seen[name] = str(members)


def test_role_kind_matches_mockup_chinfo(model):
    """계약 개정 1: 종류는 role_group 이 아니라 role_kind() 가 돌려준다."""
    for role, kind in MOCKUP_KIND.items():
        assert model.role_kind(role) == kind, role
    # 목업 CHINFO 프록시의 기본값: 모르는 역할은 '사용자 정의','main','기타'
    assert model.role_kind("알수없는역할") == "main"


# ── SpeakerConfig ─────────────────────────────────────────────────────

@pytest.mark.parametrize("preset", sorted(MOCKUP_PRESETS))
def test_from_preset_roles_and_bands(model, preset):
    config = model.SpeakerConfig.from_preset(preset)

    assert tuple(s.role for s in config.speakers) == MOCKUP_PRESETS[preset]
    assert config.preset == preset

    for speaker in config.speakers:
        kind = MOCKUP_KIND[speaker.role]
        bands = tuple(b.band for b in speaker.bands)
        assert bands, f"{speaker.role}: 대역이 비었다"
        assert set(bands) <= set(model.BANDS)
        assert speaker.size in ("small", "large", "sub")
        if kind == "sub":
            # 목업 defaultBands: 서브는 SUB 한 대역
            assert bands == ("SUB",)
            assert speaker.size == "sub"
        elif kind in ("top", "height"):          # 10-10: 탑은 종류 'top' 으로 나눔
            # 목업 defaultBands: main 이 아니면 FULL 한 대역
            assert bands == ("FULL",)
        else:
            # 메인은 목업 biampDefault 값에 따라 FULL 또는 LF/HF.
            # 계약이 그 기본값을 못 박지 않아 둘 다 허용한다.
            assert bands in (("FULL",), ("LF", "HF")), f"{speaker.role}: {bands}"
        for band in speaker.bands:
            assert band.pol in ("+", "-")

    # 출력 채널은 1부터 겹치지 않고 연속 배정된다(목업 applyPreset).
    outs = [b.out for s in config.speakers for b in s.bands]
    assert sorted(outs) == list(range(1, len(outs) + 1))


@pytest.mark.parametrize("preset", sorted(MOCKUP_PRESETS))
def test_from_preset_validates_clean(model, preset):
    assert model.SpeakerConfig.from_preset(preset).validate() == []


@pytest.mark.parametrize("preset", sorted(MOCKUP_PRESETS))
def test_from_preset_groups_cover_every_speaker(model, preset):
    config = model.SpeakerConfig.from_preset(preset)
    groups = config.groups()
    flattened = [s.role for members in groups.values() for s in members]
    assert sorted(flattened) == sorted(s.role for s in config.speakers)
    for name, members in groups.items():
        assert {model.role_group(s.role) for s in members} == {name}


def test_validate_reports_duplicate_role_and_output(model):
    config = model.SpeakerConfig.from_preset("5.1")
    config.speakers.append(model.Speaker(
        role=config.speakers[0].role, size="small",
        bands=[model.Band(band="FULL", out=config.speakers[0].bands[0].out)]))
    problems = config.validate()
    assert problems, "역할·출력 채널이 겹쳤는데 validate() 가 빈 리스트다"
    assert all(isinstance(message, str) for message in problems)


def test_set_sub_count_round_trip_0_to_8(model):
    config = model.SpeakerConfig.from_preset("5.1")
    original_subs = config.sub_count()
    assert original_subs == 1

    def non_sub_fingerprint():
        return [(s.role, s.size, tuple((b.band, b.out, b.pol) for b in s.bands))
                for s in config.speakers if s.role not in MOCKUP_SUB_ROLES]

    baseline = non_sub_fingerprint()

    config.set_sub_count(8)
    assert config.sub_count() == 8
    sub_roles = [s.role for s in config.speakers if s.role in MOCKUP_SUB_ROLES]
    assert sorted(sub_roles) == sorted(MOCKUP_SUB_ROLES)
    assert config.validate() == []
    assert non_sub_fingerprint() == baseline, "서브를 늘릴 때 메인이 바뀌었다"

    config.set_sub_count(0)
    assert config.sub_count() == 0
    assert [s.role for s in config.speakers if s.role in MOCKUP_SUB_ROLES] == []
    assert config.validate() == []
    assert non_sub_fingerprint() == baseline, "서브를 지울 때 메인이 바뀌었다"

    config.set_sub_count(original_subs)
    assert config.sub_count() == original_subs
    assert config.validate() == []
    assert non_sub_fingerprint() == baseline

    # 목업 setSubCount 의 클램프: Math.max(0, Math.min(8, n))
    config.set_sub_count(-3)
    assert config.sub_count() == 0
    config.set_sub_count(99)
    assert config.sub_count() == 8
    assert config.validate() == []


def test_set_sub_count_marks_preset_custom(model):
    config = model.SpeakerConfig.from_preset("5.1")
    config.set_sub_count(3)
    assert config.preset == "custom"       # 목업 setSubCount 마지막 줄


# ── DspProfile ────────────────────────────────────────────────────────

def test_dsp_profile_presets_match_mockup(model):
    presets = model.DspProfile.presets()
    assert set(presets) == set(MOCKUP_DSP_PROFILES)
    for key, (label, fir_taps, fir_budget, iir_bands) in MOCKUP_DSP_PROFILES.items():
        profile = presets[key]
        assert (profile.fir_taps, profile.fir_budget, profile.iir_bands) == \
            (fir_taps, fir_budget, iir_bands), key
        assert profile.support_mode in ("iir", "hybrid", "fir")
        assert profile.support_taps > 0
        assert profile.profile == key
        # 계약 개정 1: DspProfile 맨 뒤에 name 필드 추가(목업 DSP_PROFILES 의 표시 이름)
        assert profile.name == label, key


def test_dsp_profile_field_order(model):
    """계약 개정 1: name 은 맨 뒤에 추가된 필드다(위치 인자 호환)."""
    import dataclasses

    names = [f.name for f in dataclasses.fields(model.DspProfile)]
    assert names[:6] == ["profile", "fir_taps", "fir_budget", "iir_bands",
                         "support_mode", "support_taps"]
    assert names[-1] == "name"


# ── TargetCurve ───────────────────────────────────────────────────────

def _target_at_reference(points, f: float) -> float:
    """목업 targetAt(370행) 의 독립 재구현.

    JS 원문:
        const P=S.tgt.slice().sort((a,b)=>a.f-b.f);
        if(f<=P[0].f)return P[0].db;
        if(f>=P[P.length-1].f)return P[P.length-1].db;
        for(...){const a=P[i],b=P[i+1];if(f>=a.f&&f<=b.f){
          const t=(Math.log(f)-Math.log(a.f))/(Math.log(b.f)-Math.log(a.f));
          const u=t*t*(3-2*t); return a.db+(b.db-a.db)*u;}}
        return 0;

    즉 로그 주파수 위의 정규화 위치 t 에 스무스스텝 u = t²(3−2t) 를 먹인
    구간별 보간이고, 제어점 밖은 끝점 값으로 평평하다.
    """
    sorted_points = sorted(points, key=lambda p: p[0])
    if f <= sorted_points[0][0]:
        return float(sorted_points[0][1])
    if f >= sorted_points[-1][0]:
        return float(sorted_points[-1][1])
    for (a_f, a_db), (b_f, b_db) in zip(sorted_points, sorted_points[1:]):
        if a_f <= f <= b_f:
            t = (math.log(f) - math.log(a_f)) / (math.log(b_f) - math.log(a_f))
            u = t * t * (3.0 - 2.0 * t)
            return float(a_db) + (float(b_db) - float(a_db)) * u
    return 0.0


# 손 계산 근거. 모두 t 가 깔끔한 분수라 스무스스텝 값이 유리수로 떨어진다.
#
# [A] 제어점 (20,0) (200,0) (20000,−6)
#   f=10     : 최저 제어점 아래 → 끝점 0 dB
#   f=20     : 제어점 위 → 0 dB
#   f=100    : 20~200 구간, 양 끝 모두 0 dB → 보간해도 0 dB
#   f=200    : 제어점 위 → 0 dB
#   f=200·10^0.5 = 632.4555320336759 : 200~20000 구간.
#              t = ln(√10)/ln(100) = (0.5·ln10)/(2·ln10) = 1/4
#              u = (1/4)²·(3 − 1/2) = (1/16)·(5/2) = 5/32 = 0.15625
#              db = 0 + (−6 − 0)·5/32 = −30/32 = −0.9375
#   f=2000   : t = ln10/ln100 = 1/2, u = (1/4)(3−1) = 1/2
#              db = −6 · 1/2 = −3.0
#   f=200·10^1.5 = 6324.555320336759 :
#              t = (1.5·ln10)/(2·ln10) = 3/4
#              u = (9/16)·(3 − 3/2) = (9/16)·(3/2) = 27/32 = 0.84375
#              db = −6 · 27/32 = −162/32 = −5.0625
#   f=20000  : 최고 제어점 → −6 dB
#   f=48000  : 그 위 → −6 dB
HAND_CASE_A = ((20.0, 0.0), (200.0, 0.0), (20000.0, -6.0))
HAND_EXPECTED_A = (
    (10.0, 0.0),
    (20.0, 0.0),
    (100.0, 0.0),
    (200.0, 0.0),
    (632.4555320336759, -0.9375),
    (2000.0, -3.0),
    (6324.555320336759, -5.0625),
    (20000.0, -6.0),
    (48000.0, -6.0),
)

# [B] 제어점을 내림차순으로 준다 (16000,−3) (25,6) → 정렬 후 (25,6) (16000,−3).
#   기하 중점 f = √(25·16000) = √400000 = 632.4555320336759
#   t = 1/2, u = 1/2 → db = 6 + (−3 − 6)·1/2 = 6 − 4.5 = 1.5
HAND_CASE_B = ((16000.0, -3.0), (25.0, 6.0))
HAND_EXPECTED_B = ((25.0, 6.0), (632.4555320336759, 1.5), (16000.0, -3.0))

# [C] 제어점 두 개 (100,0) (10000,10). 기하 중점 f = 1000.
#   t = ln10/ln100 = 1/2, u = 1/2 → db = 0 + 10·1/2 = 5.0
HAND_CASE_C = ((100.0, 0.0), (10000.0, 10.0))
HAND_EXPECTED_C = ((1000.0, 5.0),)


@pytest.mark.parametrize(
    "points, expected",
    [(HAND_CASE_A, HAND_EXPECTED_A),
     (HAND_CASE_B, HAND_EXPECTED_B),
     (HAND_CASE_C, HAND_EXPECTED_C)],
    ids=["A-3points", "B-unsorted", "C-2points"],
)
def test_target_curve_at_matches_hand_calculation(model, points, expected):
    curve = model.TargetCurve(points=[(float(f), float(db)) for f, db in points])
    for frequency, want in expected:
        assert curve.at(frequency) == pytest.approx(want, abs=1e-9), frequency


@pytest.mark.parametrize(
    "points",
    [HAND_CASE_A, HAND_CASE_B, HAND_CASE_C,
     tuple(MOCKUP_TARGET_PRESETS["harman"]),
     tuple(MOCKUP_TARGET_PRESETS["cinema"])],
)
def test_target_curve_at_matches_mockup_reimplementation(model, points):
    """목업 targetAt 독립 재구현과 20 Hz~20 kHz 전 대역에서 일치."""
    curve = model.TargetCurve(points=[(float(f), float(db)) for f, db in points])
    for frequency in np.geomspace(5.0, 40000.0, 257):
        want = _target_at_reference(points, float(frequency))
        assert curve.at(float(frequency)) == pytest.approx(want, abs=1e-9), frequency


def test_target_curve_sample_matches_at(model):
    curve = model.TargetCurve(
        points=[(float(f), float(db)) for f, db in MOCKUP_TARGET_PRESETS["harman"]])
    frequencies = np.geomspace(20.0, 20000.0, 128)
    sampled = np.asarray(curve.sample(frequencies), dtype=np.float64)
    assert sampled.shape == frequencies.shape
    assert np.all(np.isfinite(sampled))
    expected = np.array([curve.at(float(f)) for f in frequencies])
    assert sampled == pytest.approx(expected, abs=1e-9)


def test_target_curve_presets_match_mockup(model):
    presets = model.TargetCurve.presets()
    assert set(presets) == set(MOCKUP_TARGET_PRESETS)
    for name, points in MOCKUP_TARGET_PRESETS.items():
        got = [(float(f), float(db)) for f, db in presets[name].points]
        assert got == [(float(f), float(db)) for f, db in points], name
        # f 오름차순 유지(계약)
        assert got == sorted(got)


# ── Project 저장·로드 라운드트립 ───────────────────────────────────────

def _build_project(model, name: str = "라운드트립"):
    return model.Project(
        name=name,
        config=model.SpeakerConfig.from_preset("7.1.4"),
        dsp=model.DspProfile.presets()["tesira"],
        target=model.TargetCurve.presets()["harman"],
    )


def _has_numpy_scalar(value) -> bool:
    if isinstance(value, (np.generic, np.ndarray)):
        return True
    if isinstance(value, dict):
        return any(_has_numpy_scalar(v) for v in value.values()) or \
            any(_has_numpy_scalar(k) for k in value)
    if isinstance(value, (list, tuple)):
        return any(_has_numpy_scalar(v) for v in value)
    return False


def test_project_to_dict_is_plain_json(model):
    data = _build_project(model).to_dict()
    assert not _has_numpy_scalar(data), "to_dict() 에 numpy 타입이 남았다"
    json.dumps(data, ensure_ascii=False)        # 예외 없이 직렬화되어야 한다


def test_project_defaults_match_contract(model):
    project = _build_project(model)
    # 10-10: `.secs` 판 2 — 그룹 '하이트' 가 벽 위쪽 하이트를 뜻한다(판 1 의 '하이트' = 지금의 탑, 불러올 때 이주).
    # 판 3(10-10) — 보정 설계 저역 감쇠 설정 키가 `mimo_*`(판 2 이하 `art_*`, 불러올 때 이주)
    assert project.version == 3
    # 계약 개정 1 (2026-09-16, 목업 값 우선): 목업 S.curt=[24,300].
    assert tuple(project.curtain) == (24.0, 300.0)
    assert set(project.bc) >= {"mode", "phase"}
    assert project.bc["mode"] in ("off", "upmix", "full")
    # 계약 개정 1: phase 는 'minphase'|'fir'|'mixed', 기본 'minphase' (목업 S.bc).
    assert project.bc["phase"] in ("minphase", "fir", "mixed")
    assert project.bc["phase"] == "minphase"
    assert set(project.mso) >= {"lo", "hi", "hp", "maxGain", "maxDelay", "maxPer",
                                "focus", "active", "level", "decay"}
    # 개정 8(09-24): 계산된 경로는 `extra['bass']['paths']` — `mso` 에는 설정만 있다.
    assert "paths" not in project.mso


def test_project_save_load_round_trip(model, tmp_path):
    project = _build_project(model)
    path = tmp_path / "라운드트립.secs"
    project.save(path)

    assert path.exists()
    text = path.read_text(encoding="utf-8")
    assert "라운드트립" in text, "ensure_ascii=False 로 저장해야 한다"
    assert "\n  " in text, "indent=2 로 저장해야 한다"

    loaded = model.Project.load(path)
    assert loaded.to_dict() == project.to_dict()


def test_project_from_dict_round_trip(model):
    project = _build_project(model)
    assert model.Project.from_dict(project.to_dict()).to_dict() == project.to_dict()


def test_project_preserves_unknown_keys(model, tmp_path):
    """모르는 키는 extra 에 보존되어 다시 저장할 때 살아남는다."""
    path = tmp_path / "미래버전.secs"
    _build_project(model).save(path)

    data = json.loads(path.read_text(encoding="utf-8"))
    data["미래의키"] = {"깊은": [1, 2, {"값": "보존"}]}
    data["another_future_key"] = 42
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")

    loaded = model.Project.load(path)
    assert loaded.extra.get("미래의키") == {"깊은": [1, 2, {"값": "보존"}]}
    assert loaded.extra.get("another_future_key") == 42

    again = tmp_path / "다시.secs"
    loaded.save(again)
    written = json.loads(again.read_text(encoding="utf-8"))

    # 결정 2 (리드, 2026-09-16): 재저장할 때 모르는 키는 최상위로 복원한다.
    # 그래야 상위 버전 .secs 를 왕복시켜도 그 버전이 제 키를 최상위에서 찾는다.
    assert written.get("미래의키") == {"깊은": [1, 2, {"값": "보존"}]}, \
        "모르는 최상위 키가 최상위로 복원되지 않았다"
    assert written.get("another_future_key") == 42
    assert "extra" not in written or "미래의키" not in (written.get("extra") or {}), \
        "최상위로 복원했으면 extra 밑에 중복으로 남기지 말 것"

    assert model.Project.load(again).to_dict() == loaded.to_dict()
    assert model.Project.load(again).extra == loaded.extra
