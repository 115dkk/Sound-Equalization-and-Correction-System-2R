"""10-10 사용자 요청 두 가지.

1. "하이트랑 탑 채널은 다른 채널이니까 따로 분리해야해" — 천장 탑(T*, Ltf …)은 그룹 '탑', 벽 위쪽 하이트(H*: Lfh · Rfh ·
   Lrh · Rrh)는 새 그룹 '하이트'. mdat 제목 FHL · RHL … 은 하이트로. 옛 `.secs`(판 1)의 '하이트' XO 설정은 '탑' 으로 이어 받는다.
2. "서브우퍼 측정 순서가 뒤죽박죽" — 구성에 SW2 · SW3 을 나중에 넣어도 레벨 표 · 타이밍 레퍼런스 목록은 역할 순서(SW1 바로
   뒤), 측정 순서는 서브를 맨 뒤에 몰되(10-09 사용자 결정) 서브끼리 역할 순서.
"""
from __future__ import annotations

import json
import types

import pytest

from conftest import load_engine


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


@pytest.fixture(scope="module")
def qapp(engine):
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    yield app
    app.processEvents()


# ──────────────────────────── 1. 탑 · 하이트 ────────────────────────────
def test_top_and_height_are_separate_roles_and_groups(engine):
    import secs.model as m

    for role in ('Tfl', 'Tfr', 'Tml', 'Tmr', 'Tbl', 'Tbr'):
        assert m.role_group(role) == '탑' and m.role_kind(role) == 'top', role
    heights = {'Hfl': ('Lfh', 'Left Front Height'), 'Hfr': ('Rfh', 'Right Front Height'),
               'Hbl': ('Lrh', 'Left Rear Height'), 'Hbr': ('Rrh', 'Right Rear Height')}
    for role, (code, name) in heights.items():
        assert role in m.ROLES and role in m.ROLE_POSITIONS
        assert m.role_group(role) == '하이트' and m.role_kind(role) == 'height'
        assert m.role_code(role) == code and m.role_label(role) == name
    assert m.ROLES[-4:] == ('Hfl', 'Hfr', 'Hbl', 'Hbr')          # 목업 역할 순서는 그대로, 하이트는 뒤에


@pytest.mark.parametrize('title, role', [
    # 하이트(벽 위쪽) — 10-10 까지는 FHL → Tfl, RHL → Tbl 로 탑에 섞였다
    ('FHL', 'Hfl'), ('FHR', 'Hfr'), ('RHL', 'Hbl'), ('RHR', 'Hbr'),
    ('LFH', 'Hfl'), ('RFH', 'Hfr'), ('LRH', 'Hbl'), ('RRH', 'Hbr'),
    ('HFL', 'Hfl'), ('HBR', 'Hbr'), ('Lfh 2', 'Hfl'),
    # 탑(천장)은 그대로
    ('TFL', 'Tfl'), ('TFR', 'Tfr'), ('TML', 'Tml'), ('TRL', 'Tbl'), ('TRR', 'Tbr'), ('TBL', 'Tbl'),
    ('LTF', 'Tfl'), ('RTR', 'Tbr'),
])
def test_mdat_titles_map_heights_and_tops_apart(engine, title, role):
    from secs.model import role_from_title

    assert role_from_title(title) == role


def test_config_with_tops_and_heights_has_two_xo_groups(engine):
    import secs.model as m

    config = m.SpeakerConfig.from_preset('7.1.4')
    for role in ('Hfl', 'Hfr'):
        config.speakers.append(m.Speaker(role=role, size='small',
                                         bands=[m.Band('FULL', config.next_free_out())]))
    groups = config.groups()
    assert [s.role for s in groups['탑']] == ['Tfl', 'Tfr', 'Tbl', 'Tbr']
    assert [s.role for s in groups['하이트']] == ['Hfl', 'Hfr']
    assert config.validate() == []


def _old_secs(version=None):
    """10-10 전 `.secs`: T* 그룹이 '하이트' 였고 판 번호가 1(또는 없음)."""
    import secs.model as m

    data = m.Project(config=m.SpeakerConfig.from_preset('7.1.4')).to_dict()
    data.pop('version')
    if version is not None:
        data['version'] = version
    data['xo'] = {'프론트': 80.0, '하이트': 100.0}
    data['extra'] = {'calibration': {'mode': 'mixed', 'xo': {'프론트': 90.0, '하이트': 110.0},
                                     'xo_rec': {'하이트': 120.0}}}
    return data


@pytest.mark.parametrize('version', [None, 1])
def test_old_secs_height_xo_moves_to_top(engine, tmp_path, version):
    import secs.calibration as cal
    import secs.model as m

    path = tmp_path / 'old.secs'
    path.write_text(json.dumps(_old_secs(version), ensure_ascii=False), encoding='utf-8')
    project = m.Project.load(path)
    assert project.version == m.PROJECT_VERSION == 3
    assert project.xo == {'프론트': 80.0, '탑': 100.0}
    assert list(project.xo) == ['프론트', '탑']                     # 자리 그대로
    calibration = project.extra['calibration']
    assert calibration['xo'] == {'프론트': 90.0, '탑': 110.0} and calibration['xo_rec'] == {'탑': 120.0}
    assert calibration['mode'] == 'mixed'                          # 다른 칸은 그대로
    settings = cal.CalibrationSettings.from_project(project)
    assert settings.xo == {'프론트': 90.0, '탑': 110.0} and settings.xo_rec == {'탑': 120.0}

    # 저장하면 지금 판(3) — 다시 열어도 또 옮기지 않는다
    project.save(path)
    again = m.Project.load(path)
    assert json.loads(path.read_text(encoding='utf-8'))['version'] == m.PROJECT_VERSION
    assert again.extra['calibration']['xo'] == {'프론트': 90.0, '탑': 110.0}


def test_new_secs_keeps_real_height_group(engine, tmp_path):
    """판 2 의 '하이트' 는 진짜 하이트(H*) — 옮기면 안 된다."""
    import secs.model as m

    config = m.SpeakerConfig.from_preset('7.1.4')
    config.speakers.append(m.Speaker(role='Hfl', size='small', bands=[m.Band('FULL', config.next_free_out())]))
    project = m.Project(config=config, xo={'탑': 100.0, '하이트': 90.0},
                        extra={'calibration': {'xo': {'탑': 110.0, '하이트': 95.0}}})
    path = tmp_path / 'new.secs'
    project.save(path)
    loaded = m.Project.load(path)
    assert loaded.xo == {'탑': 100.0, '하이트': 90.0}
    assert loaded.extra['calibration']['xo'] == {'탑': 110.0, '하이트': 95.0}


def test_migrate_group_keys_keeps_existing_top(engine):
    from secs.model import migrate_group_keys

    assert migrate_group_keys({'하이트': 90.0, '탑': 100.0}) == {'탑': 100.0}
    assert migrate_group_keys({'프론트': 80.0}) == {'프론트': 80.0}
    assert migrate_group_keys(None) is None


def test_old_run_cache_groups_move_to_top(engine, tmp_path):
    """10-10 전에 저장된 계산 결과 캐시도 그룹 '하이트'(T*) → '탑' — XO 보기가 화면의 '탑' 그룹을 찾는다."""
    import secs.calibration as cal

    def run(groups, xo):
        settings = cal.CalibrationSettings(xo=dict(xo), xo_rec=dict(xo))
        return types.SimpleNamespace(engine=types.SimpleNamespace(groups=dict(groups), xo=dict(xo)),
                                     result=types.SimpleNamespace(xo=dict(xo)), settings=settings)

    project_path = tmp_path / 'p.secs'
    cal.save_run_cache(project_path, run({'프론트': ['L', 'R'], '하이트': ['Tfl', 'Tfr'], 'sub': ['SW1']},
                                         {'프론트': 80.0, '하이트': 100.0}))
    old, _mimo = cal.load_run_cache(project_path)
    assert list(old.engine.groups) == ['프론트', '탑', 'sub'] and old.engine.groups['탑'] == ['Tfl', 'Tfr']
    for xo in (old.engine.xo, old.result.xo, old.settings.xo, old.settings.xo_rec):
        assert xo == {'프론트': 80.0, '탑': 100.0}

    # 새 판의 진짜 하이트는 그대로
    cal.save_run_cache(project_path, run({'탑': ['Tfl'], '하이트': ['Hfl'], 'sub': ['SW1']},
                                         {'탑': 100.0, '하이트': 90.0}))
    new, _mimo = cal.load_run_cache(project_path)
    assert list(new.engine.groups) == ['탑', '하이트', 'sub'] and new.settings.xo == {'탑': 100.0, '하이트': 90.0}


def test_step2_lists_height_roles_and_groups(qapp, engine):
    """구성 화면: 역할 드롭다운에 하이트 넷이 있고, 고르면 '하이트' 그룹이 '탑' 과 따로 생긴다."""
    import secs.model as m
    from PyQt6.QtWidgets import QLabel
    from secs.wizard.step2_config import Step2Config

    project = m.Project(config=m.SpeakerConfig.from_preset('7.1.4'))
    step = Step2Config(project)
    try:
        index = next(i for i, s in enumerate(project.config.speakers) if s.role == 'Tbl')
        combos = [c for c in step._table_combos if c.findData('Tfl') >= 0]
        assert len(combos) == len(project.config.speakers)
        for role in ('Hfl', 'Hfr', 'Hbl', 'Hbr'):
            assert combos[0].findData(role) >= 0, role
        combo = next(c for c in combos if c.currentData() == 'Tbl')
        combo.setCurrentIndex(combo.findData('Hbl'))
        qapp.processEvents()
        assert project.config.speakers[index].role == 'Hbl'
        assert set(project.config.groups()) >= {'탑', '하이트'}
        headers = [w.text().strip() for row in range(step.table.rowCount())
                   for w in [step.table.cellWidget(row, 0)] if isinstance(w, QLabel)]
        assert '탑 그룹 · 3개' in headers and '하이트 그룹 · 1개' in headers
        assert headers.index('탑 그룹 · 3개') < headers.index('하이트 그룹 · 1개')
    finally:
        step.close()


def test_group_and_kind_words_are_translated(engine):
    import secs.wizard.strings  # noqa: F401 - 위저드 번역표(en_g*)를 등록한다
    from secs.i18n import translate

    assert translate('탑', 'en') == 'Top' and translate('하이트', 'en') == 'Height'
    assert translate('  탑 그룹 · 4개', 'en') == '  Top group · 4'
    assert translate('Hfl: 메인/탑/하이트 스피커에는 SUB 대역을 쓸 수 없습니다', 'en') == \
        'Hfl: main/top/height speakers cannot use the SUB band'


# ──────────────────────────── 2. 서브 순서 ────────────────────────────
def _late_subs_project():
    """5.1 에 서브를 둘 더 — SW2 · SW3 이 `config.speakers` 끝(Ls · Rs 뒤)에 붙는다."""
    import secs.model as m

    project = m.Project(config=m.SpeakerConfig.from_preset('5.1'))
    project.config.set_sub_count(3)
    assert project.config.roles() == ['L', 'R', 'C', 'SW1', 'Ls', 'Rs', 'SW2', 'SW3']
    return project


ROLE_ORDER = ['L', 'R', 'C', 'SW1', 'SW2', 'SW3', 'Ls', 'Rs']
MEASURE_ORDER = ['L', 'R', 'C', 'Ls', 'Rs', 'SW1', 'SW2', 'SW3']     # 서브는 맨 뒤(10-09), 서브끼리 역할 순서


def test_ordered_speakers_helper(engine):
    import secs.model as m

    project = _late_subs_project()
    assert [s.role for s in m.ordered_speakers(project.config.speakers)] == ROLE_ORDER
    assert [s.role for s in m.ordered_speakers(project.config.speakers, subs_last=True)] == MEASURE_ORDER
    # 모르는 역할(사용자 정의)은 구성 순서대로 맨 뒤, 저장 순서는 그대로
    project.config.speakers.insert(0, m.Speaker(role='custom', size='small', bands=[m.Band('FULL', 20)]))
    assert [s.role for s in m.ordered_speakers(project.config.speakers)][-1] == 'custom'
    assert project.config.roles()[0] == 'custom'


def test_level_table_and_timing_list_follow_role_order(qapp, engine):
    from secs.wizard.step3_level import Step3Level
    from secs.wizard.step4_layout import Step4Layout
    from secs.wizard.timing import measurement_roles

    project = _late_subs_project()
    level = Step3Level(project)
    layout = Step4Layout(project)
    try:
        assert list(level._rows) == ROLE_ORDER
        assert [role for role, _out in measurement_roles(project)] == ROLE_ORDER
        combo = layout.timing_role_combo
        assert [combo.itemData(i) for i in range(combo.count())] == ROLE_ORDER
        # 저장 순서(.secs)는 그대로
        assert project.config.roles() == ['L', 'R', 'C', 'SW1', 'Ls', 'Rs', 'SW2', 'SW3']
    finally:
        level.close()
        layout.close()


def test_measurement_plan_keeps_subs_last_in_role_order(qapp, engine):
    from secs.rewapi.interfaces import plan_from_project
    from secs.wizard.step5_measure import Step5Measure

    project = _late_subs_project()
    step = Step5Measure(project)
    try:
        assert step.plan().roles == MEASURE_ORDER
        assert plan_from_project(project, [1]).roles == MEASURE_ORDER
    finally:
        step.close()
