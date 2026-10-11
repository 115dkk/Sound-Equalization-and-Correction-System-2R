# -*- coding: utf-8 -*-
"""계산 결과 저장·복원(10-09 사용자: 불러오기 하니 연산이 다 풀려 있다)."""
from __future__ import annotations

import pickle

from test_calibration_step import _compute, _page, _project, synth  # noqa: F401
from test_calibration_step import fast_engine  # noqa: F401
from test_wizard_common import engine, qapp  # noqa: F401


def test_run_pickles_and_reloads_on_open(qapp, synth, fast_engine, tmp_path):  # noqa: F811
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 3)
    page = _page(project, irs, tmp_path)
    try:
        run = _compute(page, qapp)
        blob = pickle.dumps((run, None))
        assert len(blob) > 0
        page._cache_thread.join(30)
        path = cal.run_cache_path(page.project_path)
        assert path is not None and path.exists()            # 계산이 끝나면 저절로 남는다
        page.run = None
        assert page.restore_cached_run()
        assert page.run is not None and page.run.target_key == run.target_key and not page.stale()
    finally:
        page.deleteLater()


def test_compute_reports_progress(qapp, synth, fast_engine, tmp_path):  # noqa: F811
    """[계산] 진행 (k/n) — 10-09 사용자: 계산 중 얼마나 됐는지 안 나온다."""
    from secs import calibration as cal

    case, irs = synth
    project = _project(case, 3)
    seen = []
    cal.compute(project, irs, cal.CalibrationSettings.from_project(project), progress=lambda k, n: seen.append((k, n)))
    assert seen and seen[0][0] >= 1 and all(n == seen[0][1] for _k, n in seen)
    assert [k for k, _n in seen] == sorted(k for k, _n in seen)
