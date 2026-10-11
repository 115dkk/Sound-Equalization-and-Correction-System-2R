"""타이밍 레퍼런스를 프로그램에서 고른다 (2026-09-18 사용자 지시).

4단계 '측정 옵션'에서 종류(음향·루프백·없음)와 기준 출력을 고르면 5단계 측정 계획과
`RealREW.configure` 까지 이어지는지 본다(2026-09-23 단계 순서를 바꾸며 장치 단계에서 옮겼다 —
기준 스피커 목록이 구성을 따라서).
"""
from __future__ import annotations

import pytest

from conftest import load_engine
from test_rewapi_real import StubSession, _client


@pytest.fixture(scope="module")
def engine():
    return load_engine("package")


@pytest.fixture(scope="module")
def qapp():
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance() or QApplication([])
    yield app
    app.processEvents()


@pytest.fixture()
def rew_mod(engine):
    import secs.rewapi.rew as module
    return module


def _project(biamp: bool = False):
    import secs.model as model

    return model.Project(config=model.SpeakerConfig.from_preset("2.1", biamp=biamp))


def test_default_is_acoustic_from_the_r_speaker(qapp, engine):
    from secs.wizard.step4_layout import Step4Layout
    from secs.wizard.timing import measurement_roles, timing_settings

    project = _project()
    step = Step4Layout(project)
    try:
        outputs = dict(measurement_roles(project))
        assert project.extra["devices"]["timing_reference"] == "acoustic"
        assert project.extra["devices"]["timing_ref_role"] == "R"
        assert timing_settings(project) == ("acoustic", outputs["R"])
        assert step.timing_role_combo.isVisibleTo(step)
        assert not step.timing_channel.isVisibleTo(step)
    finally:
        step.close()


def test_biamp_default_uses_the_high_band(qapp, engine):
    from secs.wizard.step4_layout import Step4Layout

    project = _project(biamp=True)
    step = Step4Layout(project)
    try:
        assert project.extra["devices"]["timing_ref_role"] == "R:HF"
    finally:
        step.close()


def test_choice_reaches_the_measurement_plan(qapp, engine):
    from secs.wizard.step4_layout import Step4Layout
    from secs.wizard.timing import timing_settings

    project = _project()
    step = Step4Layout(project)
    try:
        step.timing_combo.setCurrentIndex(step.timing_combo.findData("none"))
        assert timing_settings(project) == ("none", None)
        assert step.timing_none.isVisibleTo(step)

        step.timing_combo.setCurrentIndex(step.timing_combo.findData("loopback"))
        step.timing_channel.setValue(22)
        assert timing_settings(project) == ("loopback", 22)

        step.timing_combo.setCurrentIndex(step.timing_combo.findData("acoustic"))
        step.timing_role_combo.setCurrentIndex(step.timing_role_combo.findData("L"))
        kind, output = timing_settings(project)
        assert kind == "acoustic"
        assert output == project.config.speakers[0].bands[0].out

        # 다시 열어도 선택이 남는다.
        again = Step4Layout(project)
        try:
            assert again.timing_role_combo.currentData() == "L"
        finally:
            again.close()
    finally:
        step.close()


def test_step5_plan_carries_the_choice(qapp, engine, tmp_path):
    import secs.rewapi as rewapi
    from secs.wizard import WizardWindow

    project = _project()
    rew = rewapi.FakeREW(delay_s=0.0)
    window = WizardWindow(project, rew, rew, rew, rew)
    try:
        step2 = window.layout_page
        step2.timing_combo.setCurrentIndex(step2.timing_combo.findData("none"))
        plan = window.step5.plan()
        assert (plan.timing_reference, plan.timing_ref_output) == ("none", None)

        step2.timing_combo.setCurrentIndex(step2.timing_combo.findData("acoustic"))
        plan = window.step5.plan()
        assert plan.timing_reference == "acoustic"
        assert plan.timing_ref_output == plan.outputs["R"]
        assert plan.validate() == []
    finally:
        window.close()
        qapp.processEvents()


def test_configure_sends_the_ref_output_asio(rew_mod, engine):
    """ASIO: `POST /audio/asio/ref-output {"output": "2: Out 2"}` (doc.json 확인)."""
    import secs.rewapi as rewapi

    stub = StubSession(channels=["1: Out 1", "2: Out 2", "3: Out 3"], driver="ASIO")
    client = _client(rew_mod, stub)
    plan = rewapi.fake_plan(["L"], [1])
    plan.timing_reference = "acoustic"
    plan.timing_ref_output = 2
    client.configure(plan)
    assert stub.posts("/measure/timing/reference") == ["Acoustic"]
    assert stub.posts("/audio/asio/ref-output") == [{"output": "2: Out 2"}]


def test_configure_skips_the_ref_output_when_none(rew_mod, engine):
    import secs.rewapi as rewapi

    stub = StubSession(channels=["1: Out 1", "2: Out 2"], driver="ASIO")
    client = _client(rew_mod, stub)
    plan = rewapi.fake_plan(["L"], [1])
    plan.timing_reference = "none"
    plan.timing_ref_output = 2
    client.configure(plan)
    assert stub.posts("/measure/timing/reference") == ["None"]
    assert stub.posts("/audio/asio/ref-output") == []
