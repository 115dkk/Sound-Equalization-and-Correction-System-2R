"""P2 B: 측정 세션 — 결과를 `.secs` 에 기록하고 IR 을 파일로 남긴다.

`MeasurementSession` 이 위치별 측정을 돌리고
- IR 을 `<프로젝트>.irs/<role>_P<pos>.wav` 로 저장하고(soundfile, 없으면 자체 WAV 기록),
- `project.measurements` 에 요약을 넣고 측정마다 `project.save(경로)` 로 자동 저장한다.

기록 모양은 계약 개정 1(04:35) 을 따른다 — **위치당 한 항목**:
`{'pos','label','uuid':{role:uuid|None},'mdat':None,'ir':{role:wav경로},
  'spl_db':{role:float|None},'t0':{role:float},'done':bool,'sample_rate':int|None}`.
같은 위치를 다시 측정하면 항목이 교체된다.

`'t0'` 는 IR 배열 인덱스 0 의 절대 시각(초)이다. **WAV 헤더에는 담을 곳이 없어
IR 파일만으로는 복원할 수 없으므로 `.secs` 의 이 값이 진실이다.** 옛 `.secs` 에는
없을 수 있으니 읽는 쪽은 0.0 을 기본으로 봐야 한다(`t0_of`).

재시작하면 `done_positions()` 로 이미 끝난 위치를 건너뛴다. Qt 의존 없음.
"""
from __future__ import annotations

import struct
import threading
from pathlib import Path
from typing import Any, Iterable

import numpy as np

from secs.rewapi.interfaces import (
    MeasurementPlan, MeasurementResult, ProgressCallback, emit, progress,
)


# ──────────────────────────── IR 파일 ────────────────────────────
def _write_wav_float32(path: Path, data: np.ndarray, sample_rate: int) -> None:
    """soundfile 없이 32비트 float WAV 를 쓴다(모노, little-endian)."""
    samples = np.asarray(data, dtype='<f4').ravel()
    payload = samples.tobytes()
    block_align = 4
    header = b'RIFF' + struct.pack('<I', 36 + len(payload)) + b'WAVE'
    header += b'fmt ' + struct.pack('<IHHIIHH', 16, 3, 1, int(sample_rate),
                                    int(sample_rate) * block_align, block_align, 32)
    header += b'data' + struct.pack('<I', len(payload))
    path.write_bytes(header + payload)


def _read_wav_float32(path: Path) -> tuple[np.ndarray, int]:
    """`_write_wav_float32` 가 쓴 파일을 읽는다(최소 파서)."""
    raw = path.read_bytes()
    if raw[:4] != b'RIFF' or raw[8:12] != b'WAVE':
        raise ValueError(f'WAV 파일이 아닙니다: {path}')
    pos, sample_rate, bits, fmt, data = 12, 48000, 32, 3, b''
    while pos + 8 <= len(raw):
        chunk_id = raw[pos:pos + 4]
        size = struct.unpack('<I', raw[pos + 4:pos + 8])[0]
        body = raw[pos + 8:pos + 8 + size]
        if chunk_id == b'fmt ':
            fmt, _ch, sample_rate = struct.unpack('<HHI', body[:8])
            bits = struct.unpack('<H', body[14:16])[0]
        elif chunk_id == b'data':
            data = body
        pos += 8 + size + (size & 1)
    if fmt != 3 or bits != 32:
        raise ValueError(f'32비트 float WAV 가 아닙니다: {path} (fmt={fmt}, bits={bits})')
    return np.frombuffer(data, dtype='<f4').astype(np.float32), int(sample_rate)


def _read_mono_float_wav(path: Path) -> tuple[np.ndarray, int] | None:
    """모노 32비트 float WAV 면 바로 읽고, 아니면 None(→ soundfile 이 읽는다).

    측정 IR 은 이 프로그램이 쓴 모노 float WAV 뿐이다. soundfile 로 열면 파일 하나에 4 ms 가
    들어(libsndfile 열기 비용, 9.1.6 × 위치 3 = 75개 → 0.3 s, 반응 속도 09-23) 헤더만 보고
    데이터를 그대로 읽는다. float 파일의 float 읽기는 변환이 없어 soundfile 과 값이 같다.
    """
    raw = path.read_bytes()
    if raw[:4] != b'RIFF' or raw[8:12] != b'WAVE':
        return None
    pos, fmt, data = 12, None, None
    while pos + 8 <= len(raw):
        chunk_id = raw[pos:pos + 4]
        size = struct.unpack('<I', raw[pos + 4:pos + 8])[0]
        if chunk_id == b'fmt ' and size >= 16:
            fmt = struct.unpack('<HHIIHH', raw[pos + 8:pos + 24])
        elif chunk_id == b'data':
            data = raw[pos + 8:pos + 8 + size]
        pos += 8 + size + (size & 1)
    if fmt is None or data is None:
        return None
    tag, channels, rate, _byte_rate, block_align, bits = fmt
    if tag != 3 or channels != 1 or bits != 32 or block_align != 4 or len(data) % 4:
        return None
    return np.frombuffer(data, dtype='<f4').astype(np.float32), int(rate)


#: 이 프로세스가 방금 저장한 IR — (경로, 수정 시각, 크기) → (float32 배열, 샘플레이트).
#: 막 쓴 wav 를 처음 여는 데 파일마다 4 ms 가 들었다(9.1.6 × 위치 3 = 0.3 s, 6단계 진입 때
#: 화면 스레드에서 — 백신 실시간 검사로 추정, 반응 속도 09-23). 파일에 쓴 float32 값 그대로라
#: 다시 읽은 것과 비트 단위로 같다. 파일이 바뀌면(다시 측정) 키가 달라져 파일에서 읽는다.
_RECENT: dict[tuple[str, int, int], tuple[np.ndarray, int]] = {}
_RECENT_LIMIT_BYTES = 256 * 1024 * 1024
_RECENT_LOCK = threading.Lock()


def _recent_key(path: Path) -> tuple[str, int, int] | None:
    try:
        stat = path.stat()
    except OSError:
        return None
    return (str(path.resolve()), stat.st_mtime_ns, stat.st_size)


def _remember(path: Path, samples: np.ndarray, sample_rate: int) -> None:
    key = _recent_key(path)
    if key is None:
        return
    kept = np.array(samples, dtype=np.float32, copy=True)
    kept.flags.writeable = False
    with _RECENT_LOCK:
        for old in [k for k in _RECENT if k[0] == key[0]]:
            del _RECENT[old]                 # 같은 파일의 옛 값
        _RECENT[key] = (kept, int(sample_rate))
        total = sum(item[0].nbytes for item in _RECENT.values())
        while total > _RECENT_LIMIT_BYTES and len(_RECENT) > 1:
            oldest = next(iter(_RECENT))     # 넣은 순서 — 가장 오래된 것부터 버린다
            total -= _RECENT.pop(oldest)[0].nbytes


def _recall(path: Path) -> tuple[np.ndarray, int] | None:
    key = _recent_key(path)
    if key is None:
        return None
    with _RECENT_LOCK:
        item = _RECENT.get(key)
    if item is None:
        return None
    return item[0].copy(), item[1]           # 읽은 것처럼 새 배열(호출한 쪽이 고쳐도 된다)


class IrSaver:
    """`<프로젝트>.irs/` 폴더에 측정 IR 을 WAV 로 저장·복원."""

    def __init__(self, project_path: str | Path, *, use_soundfile: bool = True) -> None:
        self.project_path = Path(project_path)
        self.use_soundfile = bool(use_soundfile)

    @property
    def directory(self) -> Path:
        """`프로젝트.secs` → `프로젝트.irs`."""
        return self.project_path.with_suffix('.irs')

    def file_for(self, role: str, pos: int) -> Path:
        safe = str(role).replace(':', '_').replace('/', '_').replace('\\', '_')
        return self.directory / f'{safe}_P{int(pos)}.wav'

    def save(self, role: str, pos: int, ir: np.ndarray, sample_rate: int) -> Path:
        target = self.file_for(role, pos)
        target.parent.mkdir(parents=True, exist_ok=True)
        samples = np.asarray(ir, dtype=np.float32).ravel()
        written = False
        if self.use_soundfile:
            try:
                import soundfile as sf  # 지연 임포트(선택 의존)
                sf.write(str(target), samples, int(sample_rate), subtype='FLOAT')
                written = True
            except Exception:  # noqa: BLE001 - soundfile 없거나 실패하면 자체 기록
                pass
        if not written:
            _write_wav_float32(target, samples, int(sample_rate))
        _remember(target, samples, int(sample_rate))
        return target

    def load(self, role: str, pos: int) -> tuple[np.ndarray, int]:
        return self.load_path(self.file_for(role, pos))

    @staticmethod
    def load_path(path: str | Path) -> tuple[np.ndarray, int]:
        p = Path(path)
        recent = _recall(p)
        if recent is not None:
            return recent
        try:
            fast = _read_mono_float_wav(p)
        except (OSError, struct.error):
            fast = None                  # 읽기 문제는 아래 soundfile 경로가 원래대로 알린다
        if fast is not None:
            return fast
        try:
            import soundfile as sf  # 지연 임포트
            data, rate = sf.read(str(p), dtype='float32', always_2d=False)
            if getattr(data, 'ndim', 1) > 1:
                data = data[:, 0]
            return np.asarray(data, dtype=np.float32), int(rate)
        except Exception:  # noqa: BLE001 - soundfile 없으면 자체 파서
            return _read_wav_float32(p)


# ──────────────────────────── 기록 모양(계약 개정 1) ────────────────────────────
def _pos_of(record: Any) -> int | None:
    try:
        return int(record.get('pos'))
    except (AttributeError, TypeError, ValueError):
        return None


def _repair_entry(entry: dict, pos: int, label: str) -> dict:
    """외부에서 만들어진 항목도 개정 1 모양을 갖추게 한다."""
    entry['pos'] = int(pos)
    entry.setdefault('label', label)
    entry.setdefault('mdat', None)
    for key in ('uuid', 'ir', 'spl_db', 't0'):
        if not isinstance(entry.get(key), dict):
            entry[key] = {}
    entry.setdefault('done', False)
    entry.setdefault('sample_rate', None)
    return entry


# ──────────────────────────── 세션 ────────────────────────────
class MeasurementSession:
    """측정 실행 + `.secs` 기록.

    제품 경로는 위저드 5단계 워커가 **위치마다** `run_position` 을 부른다(위치 사이에
    사용자가 마이크를 옮긴다). `run_plan` 은 전 위치를 한 번에 도는 테스트·스크립트용이다.
    """

    def __init__(self, project: Any, measurer: Any,
                 ir_saver: Any = None, *,
                 project_path: str | Path | None = None,
                 auto_save: bool = True) -> None:
        self.project = project
        self.measurer = measurer
        self.auto_save = bool(auto_save)

        if isinstance(ir_saver, (str, Path)):     # 경로를 바로 준 경우
            project_path = project_path or ir_saver
            ir_saver = IrSaver(project_path)
        if ir_saver is None:
            if project_path is None:
                raise ValueError('ir_saver 나 project_path 중 하나는 있어야 합니다')
            ir_saver = IrSaver(project_path)
        if project_path is None:
            project_path = getattr(ir_saver, 'project_path', None)

        self.ir_saver = ir_saver
        self.project_path = Path(project_path) if project_path is not None else None
        self._lock = threading.Lock()

    # ── 기록 ──
    def records(self) -> list[dict]:
        return list(getattr(self.project, 'measurements', []) or [])

    def entry_for(self, pos: int) -> dict | None:
        """위치 하나의 기록(개정 1: 위치당 한 항목). 없으면 None."""
        for rec in self.records():
            try:
                if int(rec.get('pos', -1)) == int(pos):
                    return rec
            except (TypeError, ValueError):
                continue
        return None

    def done_positions(self, plan: MeasurementPlan | None = None) -> list[int]:
        """측정이 끝난 위치 번호. plan 이 있으면 그 모든 role 이 있는 위치만."""
        out: list[int] = []
        needed = set(plan.roles) if plan is not None else None
        for rec in self.records():
            try:
                pos = int(rec.get('pos', -1))
            except (TypeError, ValueError):
                continue
            irs = rec.get('ir') if isinstance(rec.get('ir'), dict) else {}
            roles = {str(role) for role, path in irs.items() if path}
            if needed is not None and not needed <= roles:
                continue
            if needed is None and not bool(rec.get('done')):
                continue
            out.append(pos)
        return sorted(set(out))

    def pending_positions(self, plan: MeasurementPlan) -> list[int]:
        done = set(self.done_positions(plan))
        return [p for p in plan.positions if p not in done]

    def _new_entry(self, pos: int, plan: MeasurementPlan | None) -> dict:
        return {
            'pos': int(pos),
            'label': self.label_for(pos, plan),
            'uuid': {},
            'mdat': None,
            'ir': {},
            'spl_db': {},
            't0': {},
            'done': False,
            'sample_rate': None,
        }

    def label_for(self, pos: int, plan: MeasurementPlan | None = None) -> str:
        """목업 5단계의 위치 표시 문구."""
        return f'위치 {int(pos)}'

    def record(self, result: MeasurementResult, plan: MeasurementPlan | None = None) -> dict:
        """결과 하나를 IR 파일로 저장하고 위치 항목에 반영(자동 저장 포함).

        개정 1 모양: 위치당 한 항목, `uuid`·`ir`·`spl_db`·`t0` 는 role 사전이고
        `sample_rate` 는 위치 하나에 한 값(마지막 결과의 값으로 덮어쓴다).
        """
        path = self.ir_saver.save(result.role, result.pos, result.ir, result.sample_rate)
        role = str(result.role)
        with self._lock:
            existing = getattr(self.project, 'measurements', None)
            if existing is None:
                self.project.measurements = existing = []
            index = next((i for i, rec in enumerate(existing)
                          if _pos_of(rec) == int(result.pos)), None)
            if index is None:
                entry = self._new_entry(result.pos, plan)
                existing.append(entry)
            else:
                entry = existing[index]
                _repair_entry(entry, result.pos, self.label_for(result.pos, plan))
            entry['uuid'][role] = result.uuid
            entry['ir'][role] = self._relative(path)
            entry['spl_db'][role] = (None if result.spl_db is None else float(result.spl_db))
            # WAV 로는 복원 못 하는 값이라 여기 남긴 것이 진실이다.
            entry['t0'][role] = float(getattr(result, 't0_s', 0.0) or 0.0)
            offset = getattr(result, 'spl_offset_db', None)
            if offset is not None:                      # mdat 로 다시 쓸 때의 SPL 기준(10-09)
                entry.setdefault('spl_offset', {})[role] = float(offset)
            entry['sample_rate'] = int(result.sample_rate)
            if plan is not None:
                entry['done'] = all(entry['ir'].get(r) for r in plan.roles)
            else:
                entry['done'] = bool(entry['ir'])
            if self.auto_save:
                self.save()
        return entry

    def start_position(self, pos: int, plan: MeasurementPlan | None = None) -> dict:
        """한 위치를 다시 측정하기 전에 그 항목을 비운다(재측정 시 교체)."""
        with self._lock:
            existing = getattr(self.project, 'measurements', None)
            if existing is None:
                self.project.measurements = existing = []
            index = next((i for i, rec in enumerate(existing)
                          if _pos_of(rec) == int(pos)), None)
            entry = self._new_entry(pos, plan)
            if index is None:
                existing.append(entry)
            else:
                existing[index] = entry
        return entry

    def save(self) -> None:
        """`project.save()` — 경로를 받는 모델이면 세션의 경로로 부른다."""
        if self.project_path is None:
            self.project.save()
            return
        try:
            self.project.save(self.project_path)
        except TypeError:
            self.project.save()

    def _relative(self, path: Path) -> str:
        """`.secs` 에는 프로젝트 폴더 기준 상대 경로로 남긴다."""
        if self.project_path is None:
            return str(path)
        try:
            return str(Path(path).relative_to(self.project_path.parent)).replace('\\', '/')
        except ValueError:
            return str(path)

    def ir_path_of(self, role: str, pos: int) -> Path:
        """기록에 남은 IR 파일 경로(상대 경로면 프로젝트 폴더 기준)."""
        entry = self.entry_for(pos)
        raw = None
        if entry is not None and isinstance(entry.get('ir'), dict):
            raw = entry['ir'].get(str(role))
        if not raw:
            return self.ir_saver.file_for(role, pos)
        path = Path(raw)
        if not path.is_absolute() and self.project_path is not None:
            path = self.project_path.parent / path
        return path

    def ir_of(self, role: str, pos: int) -> tuple[np.ndarray, int]:
        """저장된 IR 을 파일에서 되읽는다."""
        return IrSaver.load_path(self.ir_path_of(role, pos))

    def t0_of(self, role: str, pos: int) -> float:
        """IR 시간 원점(초). WAV 에 없는 값이라 `.secs` 의 `'t0'` 에서만 온다.

        기록이 없는 옛 `.secs` 는 0.0 으로 본다.
        """
        entry = self.entry_for(pos)
        if entry is None or not isinstance(entry.get('t0'), dict):
            return 0.0
        value = entry['t0'].get(str(role))
        try:
            return float(value)
        except (TypeError, ValueError):
            return 0.0

    def t0_by_role(self, pos: int) -> dict[str, float]:
        """한 위치의 role → t0(초). `irs_by_role` 과 같은 role 집합을 돌려준다."""
        entry = self.entry_for(pos)
        if entry is None or not isinstance(entry.get('ir'), dict):
            return {}
        return {str(role): self.t0_of(role, pos)
                for role, stored in entry['ir'].items() if stored}

    def irs_by_role(self, pos: int) -> dict[str, tuple[np.ndarray, int]]:
        """한 위치의 role → (IR, sample_rate). `generate_filters_from_arrays` 입력용."""
        entry = self.entry_for(pos)
        if entry is None or not isinstance(entry.get('ir'), dict):
            return {}
        return {str(role): self.ir_of(role, pos)
                for role, stored in entry['ir'].items() if stored}

    # ── 실행 ──
    def run_position(self, plan: MeasurementPlan, pos: int,
                     on_progress: ProgressCallback | None = None,
                     cancel: threading.Event | None = None) -> list[MeasurementResult]:
        cancel = cancel or threading.Event()
        results = self.measurer.run_position(plan, pos, on_progress, cancel)
        for index, result in enumerate(results):
            if index == 0:
                self.start_position(pos, plan)   # 재측정이면 그 위치 항목을 갈아 끼운다
            self.record(result, plan)
        return results

    def run_plan(self, plan: MeasurementPlan,
                 on_progress: ProgressCallback | None = None,
                 cancel: threading.Event | None = None,
                 positions: Iterable[int] | None = None,
                 skip_done: bool = True) -> list[MeasurementResult]:
        """계획 전체를 위치 순서대로 측정(테스트·스크립트 전용).

        위저드는 위치마다 `run_position` 을 부른다. 취소되면 그때까지의 결과를 돌려준다.
        """
        cancel = cancel or threading.Event()
        todo = list(positions) if positions is not None else (
            self.pending_positions(plan) if skip_done else list(plan.positions))
        done: list[MeasurementResult] = []
        for pos in todo:
            if cancel.is_set():
                emit(on_progress, progress('', pos, 'cancelled', 0.0, '측정을 취소했습니다'))
                break
            got = self.run_position(plan, pos, on_progress, cancel)
            done.extend(got)
            if len(got) < len(plan.roles):   # 취소로 중간에 끊긴 위치
                break
        return done


# ──────────────────────────── IR 읽기(.secs 기록 → 위치별 IR) ────────────────────────────
# 2026-10-10 슬림화로 옛 설계 엔진(`secs.design`)에서 옮겼다 — 쓰는 곳: 위저드 창(IR 캐시)·5단계·보정 설계.
def load_project_irs(project: Any,
                     project_path: str | Path | None = None, *,
                     cache: dict | None = None
                     ) -> dict[int, dict[str, tuple[np.ndarray, int]]]:
    """`.secs` 의 `measurements` 기록에서 위치별 IR 을 읽어 온다.

    기록 모양은 P2 계약 개정 1(위치당 한 항목, `ir` 은 role → wav 경로)이고
    경로는 위의 `IrSaver` 가 쓴 규칙(`<프로젝트>.irs/<role>_P<pos>.wav`,
    `.secs` 에는 프로젝트 폴더 기준 상대 경로)을 그대로 따른다.

    `cache`(위저드 창이 든 dict)를 주면 파일마다 (경로, 수정 시각, 크기) 로 읽은 배열을
    기억해 측정·보정 설계·내보내기가 같은 wav 를 두 번 열지 않는다(반응 속도 09-23). 다시 재서 파일이
    바뀌면 키가 달라져 새로 읽고, 이번에 안 쓴 옛 항목은 버린다. 나눠 쓰는 배열이라
    읽기 전용으로 둔다 — 어느 엔진이 제자리에서 고치면 다른 엔진 결과가 바뀌기 때문이다.
    """
    def read(path: Path) -> tuple[np.ndarray, int]:
        if cache is None:
            return IrSaver.load_path(path)
        try:
            stat = path.stat()
            key = (str(path), stat.st_mtime_ns, stat.st_size)
        except OSError:
            return IrSaver.load_path(path)     # 없는 파일은 원래 경로가 오류를 낸다
        used.add(key)
        if key not in cache:
            ir, rate = IrSaver.load_path(path)
            ir = np.asarray(ir)
            ir.flags.writeable = False
            cache[key] = (ir, rate)
        return cache[key]

    used: set = set()
    base = Path(project_path).parent if project_path is not None else Path('.')
    out: dict[int, dict[str, tuple[np.ndarray, int]]] = {}
    for record in list(getattr(project, 'measurements', []) or []):
        if not isinstance(record, dict):
            continue
        try:
            pos = int(record.get('pos'))
        except (TypeError, ValueError):
            continue
        stored = record.get('ir')
        if not isinstance(stored, dict):
            continue
        # 시간 원점(REW start_time_s). B 가 `.secs` 에 role 사전으로 남긴다.
        origins = record.get('t0') if isinstance(record.get('t0'), dict) else {}
        for role, raw in stored.items():
            if not raw:
                continue
            path = Path(raw)
            if not path.is_absolute():
                path = base / path
            ir, rate = read(path)
            try:
                t0 = float(origins.get(str(role), 0.0) or 0.0)
            except (TypeError, ValueError):
                t0 = 0.0
            # 시간 원점이 0 이면 예전 그대로 2-튜플로 돌려준다(기존 호출부 호환).
            out.setdefault(pos, {})[str(role)] = ((ir, rate) if t0 == 0.0
                                                  else (ir, rate, t0))
    if cache is not None:
        for stale in [key for key in cache if key not in used]:
            del cache[stale]
    return {pos: out[pos] for pos in sorted(out)}


__all__ = ['MeasurementSession', 'IrSaver', 'load_project_irs']
