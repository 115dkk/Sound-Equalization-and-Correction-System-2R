"""6차안에서 오른쪽 액체 흐름을 정리한 확정 logo_b8.svg와 소형판을 만든다.

막대 윗면/아랫면과 기존 흰 덩어리의 끝점을 기준으로 얇은 액체 막을 계산한다.
``frame(u)``는 정지 산출과 앞 시안용 큰 판을 반환한다. u=0은 목이 늘어진
매달린 방울과 막 떨어져 나온 방울이 함께 보이는 정지 산출 프레임이다. 이후에는
부풀기→끊김→가속 낙하→착지와 튀김→복귀를 이어 한 주기를 만든다.
``anim_frame(u)``는 정지 산출과 분리된 시안2 전용 한 주기다. 방울 하나가
천천히 커진 뒤 끊겨 떨어지고 웅덩이에 흡수되는 동작만 담는다.

실행: .venv-secs\\Scripts\\python.exe tools\\make_logo_b8.py
저프레임 시안: 위 명령 뒤에 ``--draft12``를 붙인다.
시안2 8프레임: 위 명령 뒤에 ``--draft8-v2``를 붙인다.
앱용 확정 24프레임: 위 명령 뒤에 ``--frames``를 붙인다.
"""
from __future__ import annotations

import argparse
import base64
import json
import math
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "src/secs/wizard/assets"
REPORT = ROOT / "보고서/09 지난 보고서/목업·디자인·로고/로고 8차안 2026-09-24"
B6 = ASSETS / "logo_b6.svg"
B6_SMALL = ASSETS / "logo_b6_small.svg"

BAR_TOP = 28.4
BAR_BOTTOM = 35.6
FILM_START_TOP = 44.4
FILM_START_BOTTOM = 45.0
FILM_END = 52.1
DROP_X = 52.4
PUDDLE_Y = 52.5
WHITE = "#ffffff"
SHADE = "#d9e0df"
PUDDLE_PATH = (
    '  <path fill="#fff" d="M45.5 53.5 C46.4 52.3 48.1 52 49.6 52.3 '
    'C51 51.5 53 51.8 53.8 52.7 C55.5 52.6 57 53.2 56.7 54.4 '
    'C56.5 55.4 54.9 55.7 53.7 55.4 C51.6 56 48.6 56 47 55.4 '
    'C45.7 55.4 44.8 54.5 45.5 53.5Z"/>'
)


def _clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    return max(low, min(high, value))


def _smoothstep(value: float) -> float:
    value = _clamp(value)
    return value * value * (3.0 - 2.0 * value)


def _ease_in(value: float) -> float:
    value = _clamp(value)
    return value * value


def _segment(u: float, start: float, end: float) -> float:
    return _clamp((u - start) / (end - start))


def _fmt(value: float) -> str:
    return f"{value:.2f}".rstrip("0").rstrip(".")


def _point(point: tuple[float, float]) -> str:
    return f"{_fmt(point[0])} {_fmt(point[1])}"


def _smooth_path(points: list[tuple[float, float]], tension: float = 0.52) -> str:
    """닫힌 점열을 Catmull-Rom 계열 베지어 경로로 바꾼다."""
    out = [f"M{_point(points[0])}"]
    count = len(points)
    for index in range(count):
        p0 = points[(index - 1) % count]
        p1 = points[index]
        p2 = points[(index + 1) % count]
        p3 = points[(index + 2) % count]
        c1 = (p1[0] + (p2[0] - p0[0]) * tension / 3,
              p1[1] + (p2[1] - p0[1]) * tension / 3)
        c2 = (p2[0] - (p3[0] - p1[0]) * tension / 3,
              p2[1] - (p3[1] - p1[1]) * tension / 3)
        out.append(f"C{_point(c1)} {_point(c2)} {_point(p2)}")
    return "".join(out) + "Z"


def _film(edge_y: float, start_x: float, end_x: float, side: int,
          start_thickness: float, end_thickness: float,
          samples: int = 8) -> str:
    """막대 모서리에 겹쳐 붙고 끝으로 갈수록 가늘어지는 액체 막."""
    outer: list[tuple[float, float]] = []
    inner: list[tuple[float, float]] = []
    for index in range(samples + 1):
        p = index / samples
        x = start_x + (end_x - start_x) * p
        thickness = end_thickness + (start_thickness - end_thickness) * (1 - p) ** 1.18
        # 양 끝은 잔잔하고 중간만 불규칙하게 만들어 기존 평면 톤을 유지한다.
        wobble = math.sin(math.pi * p) * (
            0.10 * math.sin(5 * math.pi * p + 0.35)
            + 0.045 * math.sin(9 * math.pi * p)
        )
        outer.append((x, edge_y + side * (thickness + wobble)))
        # 막대 쪽 경계는 0.14만큼 겹쳐 축소 렌더에서도 틈이 생기지 않는다.
        inner.append((x, edge_y - side * 0.14))
    return _smooth_path(outer + inner[::-1], tension=0.42)


def _upper_film(*, small: bool = False) -> str:
    """막대 윗면에 붙어 흐르며, 불규칙한 윗면과 둥근 끝을 가진 액체 줄기."""
    if small:
        return (
            "M43.7 28.5 C45.6 28.42 48.6 28.43 51.55 28.45 "
            "C52.35 28.47 52.86 28.08 52.76 27.5 "
            "C52.66 26.92 52.08 26.66 51.55 26.92 "
            "C51.12 27.14 50.72 27.14 50.28 27.22 "
            "C49.72 27.32 49.38 27.66 48.82 27.52 "
            "C48.25 27.38 47.92 27.08 47.34 27.23 "
            "C46.74 27.39 46.48 27.67 45.92 27.48 "
            "C45.18 27.23 44.55 27.12 44.08 27.56 "
            "C43.8 27.82 43.62 28.25 43.7 28.5Z"
        )
    return (
        "M44.35 28.5 C46.16 28.44 48.73 28.45 51.58 28.46 "
        "C52.27 28.47 52.7 28.13 52.62 27.64 "
        "C52.55 27.17 52.08 26.94 51.65 27.13 "
        "C51.28 27.3 50.93 27.29 50.54 27.35 "
        "C50.02 27.43 49.73 27.7 49.23 27.57 "
        "C48.72 27.43 48.45 27.18 47.95 27.29 "
        "C47.42 27.41 47.17 27.68 46.67 27.54 "
        "C46.02 27.35 45.47 27.17 44.93 27.43 "
        "C44.5 27.63 44.2 28.11 44.35 28.5Z"
    )


def _lower_film(*, small: bool = False) -> str:
    """막대 아랫면에서 끝으로 갈수록 굵어져 방울로 흘러드는 액체 줄기."""
    start = 44.15 if small else 44.72
    return (
        f"M{_fmt(start)} 35.54 C46.28 35.56 48.43 35.56 50.18 35.57 "
        "C51.35 35.57 52.44 35.58 53.36 35.91 "
        "C52.83 36.14 52.36 36.3 51.88 36.4 "
        "C51.48 36.48 51.14 36.53 50.83 36.58 "
        "C50.56 36.62 50.38 36.88 50.13 36.86 "
        "C49.87 36.84 49.8 36.58 49.51 36.55 "
        "C49.05 36.51 48.65 36.51 48.23 36.49 "
        "C47.93 36.48 47.76 36.73 47.51 36.7 "
        "C47.26 36.68 47.2 36.44 46.92 36.4 "
        f"C46.02 36.27 45.27 36.12 {_fmt(start)} 35.96 "
        f"C{_fmt(start - 0.26)} 35.87 {_fmt(start - 0.25)} 35.61 {_fmt(start)} 35.54Z"
    )


def _pendant() -> str:
    """x≈53에서 가는 목 아래 무거운 머리가 매달린 두 봉우리 방울."""
    return (
        "M51.3 35.57 "
        "C52.2 35.54 53.28 35.54 54.25 35.58 "
        "C55.05 35.61 55.62 35.83 55.78 36.25 "
        "C55.95 36.71 55.72 37.21 55.27 37.53 "
        "C54.85 37.84 54.34 37.91 53.95 37.68 "
        "C53.62 37.48 53.47 37.14 53.34 36.85 "
        "C53.26 37.45 53.27 37.9 53.52 38.25 "
        "C53.86 38.72 54.03 39.26 53.83 39.75 "
        "C53.63 40.22 53.16 40.49 52.68 40.38 "
        "C52.14 40.26 51.83 39.76 51.92 39.26 "
        "C52 38.79 52.36 38.49 52.58 38.12 "
        "C52.78 37.8 52.75 37.34 52.69 36.88 "
        "C52.61 36.5 52.35 36.33 51.88 36.25 "
        "C51.43 36.17 51.14 35.82 51.3 35.57Z"
    )


def _falling_drop(*, top: float, stretch: float = 1.0, width: float = 1.0,
                  opacity: float = 1.0, trail_top: float | None = None,
                  trail_opacity: float = 0.36) -> str:
    """눈물형 방울과 위쪽 잔상을 같은 좌표계에서 그린다."""
    opacity = _clamp(opacity)

    def x(offset: float) -> str:
        return _fmt(DROP_X + offset * width)

    def y(offset: float) -> str:
        return _fmt(top + offset * stretch)

    drop = (
        f"M{x(0)} {y(0)} "
        f"C{x(-0.04)} {y(0.33)} {x(-0.53)} {y(0.7)} {x(-0.6)} {y(1.14)} "
        f"C{x(-0.7)} {y(1.68)} {x(-0.25)} {y(2.02)} {x(0.18)} {y(2)} "
        f"C{x(0.7)} {y(1.98)} {x(0.87)} {y(1.49)} {x(0.69)} {y(1.1)} "
        f"C{x(0.54)} {y(0.75)} {x(0.18)} {y(0.35)} {x(0)} {y(0)}Z"
    )
    shade = (
        f"M{x(-0.58)} {y(1.43)} "
        f"C{x(-0.35)} {y(1.86)} {x(0.02)} {y(2.01)} {x(0.36)} {y(1.91)} "
        f"C{x(0.59)} {y(1.84)} {x(0.72)} {y(1.66)} {x(0.78)} {y(1.47)} "
        f"C{x(0.76)} {y(1.75)} {x(0.54)} {y(1.96)} {x(0.14)} {y(1.99)} "
        f"C{x(-0.29)} {y(2.01)} {x(-0.62)} {y(1.8)} {x(-0.58)} {y(1.43)}Z"
    )
    trail = ""
    if trail_top is not None and trail_opacity > 0:
        trail = (
            f'  <path d="M{x(-0.04)} {_fmt(trail_top)} L{x(-0.04)} {y(-0.16)}" '
            f'fill="none" stroke="#ffffff" stroke-width=".16" stroke-linecap="round" opacity="{_fmt(trail_opacity)}"/>\n'
            f'  <path d="M{x(0.27)} {_fmt(trail_top + 0.2)} L{x(0.27)} {y(-0.12)}" '
            f'fill="none" stroke="#ffffff" stroke-width=".1" stroke-linecap="round" opacity="{_fmt(trail_opacity * .75)}"/>\n'
        )
    return (
        trail
        + f'  <g opacity="{_fmt(opacity)}">\n'
        + f'    <path fill="#f7f9f8" d="{drop}"/>\n'
        + f'    <path fill="#d9e0df" opacity=".9" d="{shade}"/>\n'
        + f'    <ellipse cx="{x(-0.22)}" cy="{y(0.82)}" rx="{_fmt(.13 * width)}" ry="{_fmt(.1 * stretch)}" fill="#ffffff"/>\n'
        + "  </g>"
    )


def _pendant_markup(scale_x: float = 1.0, scale_y: float = 1.0,
                    opacity: float = 1.0) -> str:
    """매달린 원액과 기존 명암을 막대 아랫면에 고정해 함께 변형한다."""
    transform = ""
    if abs(scale_x - 1.0) > 1e-6 or abs(scale_y - 1.0) > 1e-6:
        transform = (
            f' transform="translate(53 35.57) scale({_fmt(scale_x)} {_fmt(scale_y)}) '
            'translate(-53 -35.57)"'
        )
    return f"""  <g{transform} opacity="{_fmt(opacity)}">
    <path fill="url(#b8-drop)" d="{_pendant()}"/>
    <path fill="#0d2625" opacity=".12" d="M51.48 35.62 C52.55 35.52 54.48 35.54 55.55 35.72 C54.52 35.84 52.55 35.86 51.48 35.77Z"/>
    <path fill="#d9e0df" opacity=".9" d="M51.96 39.17 C52.22 39.83 52.66 40.17 53.13 40.12 C53.46 40.08 53.7 39.87 53.84 39.57 C53.82 40.08 53.45 40.45 52.93 40.48 C52.38 40.5 51.91 40.03 51.96 39.17Z"/>
    <path fill="#d9e0df" opacity=".72" d="M53.91 37.31 C54.33 37.65 54.82 37.67 55.25 37.42 C55.03 37.82 54.48 38.03 54.03 37.83 C53.88 37.76 53.76 37.66 53.67 37.53 C53.76 37.48 53.84 37.4 53.91 37.31Z"/>
    <ellipse cx="52.3" cy="38.84" rx=".22" ry=".15" fill="#fff"/>
    <ellipse cx="52.02" cy="36.22" rx=".27" ry=".16" fill="#fff"/>
  </g>"""


def _remove_main_dots(svg: str) -> str:
    targets = (("46.8", "19.3", "1.55"), ("50.5", "22.2", "1.1"),
               ("49.1", "26.3", ".9"), ("47.7", "40.8", "2"),
               # 2026-09-24 사용자 확정: 낙하 축 주변의 둥근 장식은 액체 흐름을
               # 두 갈래처럼 보이게 하므로 지우고, 눈물형 낙하 방울 하나만 남긴다.
               ("50.8", "43.6", "1.25"))
    for cx, cy, radius in targets:
        pattern = (
            rf'\s*<circle cx="{re.escape(cx)}" cy="{re.escape(cy)}" '
            rf'r="{re.escape(radius)}" fill="#fff"/>'
        )
        svg, count = re.subn(pattern, "", svg, count=1)
        if count != 1:
            raise ValueError(f"지울 원을 찾지 못함: ({cx}, {cy}, {radius})")
    lower_blob = (
        '  <path fill="#fff" d="M53.2 42.4 C54.7 41.1 56.7 42.2 56.5 44 '
        'C56.4 45.2 55.2 46.1 54 45.7 C52.7 45.3 52.2 43.4 53.2 42.4Z"/>'
    )
    if lower_blob not in svg:
        raise ValueError("지울 오른쪽 아래 둥근 방울을 찾지 못함")
    svg = svg.replace(lower_blob, "", 1)
    return svg


def _insert_before(svg: str, needle: str, markup: str) -> str:
    if needle not in svg:
        raise ValueError(f"삽입 기준을 찾지 못함: {needle[:48]}")
    return svg.replace(needle, markup + "\n" + needle, 1)


def frame(u: float) -> str:
    """0 <= u < 1인 한 주기의 큰 로고 SVG를 반환한다. 정지 산출은 u=0."""
    if not 0 <= u < 1:
        raise ValueError("u는 0 이상 1 미만이어야 합니다")
    svg = _remove_main_dots(B6.read_text(encoding="utf-8"))
    defs = """    <linearGradient id="b8-film" gradientUnits="userSpaceOnUse" x1="45" y1="27" x2="52" y2="29">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset=".7" stop-color="#f6f8f7"/>
      <stop offset="1" stop-color="#d9e0df"/>
    </linearGradient>
    <radialGradient id="b8-drop" gradientUnits="userSpaceOnUse" cx="52.25" cy="37.1" r="3.7">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset=".7" stop-color="#f7f9f8"/>
      <stop offset="1" stop-color="#f3f6f5"/>
    </radialGradient>"""
    svg = _insert_before(svg, "  </defs>", defs)

    pendant_x = 1.0
    pendant_y = 1.0
    puddle_x = 1.0
    drop = ""
    neck_residue = ""
    surface = ""

    if u == 0:
        # 확정 정지 로고와 픽셀까지 같은 시작 자세다.
        drop = _falling_drop(top=41.0, trail_top=40.18)
    elif u < 0.45:
        # 느린 ease-in: 아래 방울이 묵직해지고 목이 함께 늘어난다.
        t = _ease_in(_segment(u, 0.0, 0.45))
        pendant_x = 1.0 + 0.04 * t
        pendant_y = 1.0 + 0.12 * t
        drop = _falling_drop(
            top=41.0 + 0.65 * t,
            stretch=1.0 + 0.1 * t,
            width=1.0 + 0.08 * t,
            trail_top=40.18 + 0.1 * t,
            trail_opacity=0.36 + 0.06 * t,
        )
    elif u < 0.55:
        # 목이 끊어지는 0.10 구간. 끝의 짧은 선은 위로 튕긴 잔여분이다.
        t = _smoothstep(_segment(u, 0.45, 0.55))
        pendant_x = 1.04 - 0.025 * t
        pendant_y = 1.12 - 0.15 * t
        drop = _falling_drop(
            top=41.65 + 0.45 * t,
            stretch=1.1 + 0.08 * t,
            width=1.08 - 0.04 * t,
            trail_top=40.28 + 0.5 * t,
            trail_opacity=0.42 * (1.0 - t) ** 2,
        )
        residue = 1.0 - t
        neck_residue = (
            f'  <path d="M52.48 39.88 L52.44 {_fmt(40.45 - 0.36 * t)}" '
            f'fill="none" stroke="#f7f9f8" stroke-width="{_fmt(.16 * residue + .04)}" '
            f'stroke-linecap="round" opacity="{_fmt(.5 * residue)}"/>'
        )
    elif u < 0.80:
        # 낙하 거리는 t², 세로 늘어남과 잔상 길이는 속도에 비례한다.
        t = _segment(u, 0.55, 0.80)
        fall = t * t
        pendant_y = 0.97 - 0.035 * math.sin(math.pi * _clamp(t * 1.35))
        top = 42.1 + (49.9 - 42.1) * fall
        drop = _falling_drop(
            top=top,
            stretch=1.08 + 0.2 * t,
            width=1.04 - 0.06 * t,
            trail_top=top - (0.85 + 1.55 * t),
            trail_opacity=0.18 + 0.2 * t,
        )
        shadow_rx = 1.28 - 0.58 * t
        shadow_ry = 0.27 - 0.1 * t
        surface = (
            f'  <ellipse cx="52.35" cy="52.76" rx="{_fmt(shadow_rx)}" '
            f'ry="{_fmt(shadow_ry)}" fill="#0d2625" opacity="{_fmt(.07 + .27 * t)}"/>'
        )
    elif u < 0.95:
        # 접촉 직후 방울은 납작해져 흡수되고, 웅덩이·튀김·파문이 한 번 반응한다.
        t = _segment(u, 0.80, 0.95)
        pendant_y = 0.98 + 0.02 * _smoothstep(t)
        puddle_x = 1.0 + 0.08 * math.sin(math.pi * t)
        if t < 0.4:
            merge = _smoothstep(t / 0.4)
            drop = _falling_drop(
                top=49.9 + 0.55 * merge,
                stretch=1.28 - 0.84 * merge,
                width=0.98 + 0.72 * merge,
                opacity=1.0 - merge,
            )
        arc = 4.0 * t * (1.0 - t)
        splash_opacity = math.sin(math.pi * t) * 0.92
        surface = f"""  <ellipse cx="{_fmt(50.7 - 1.65 * t)}" cy="{_fmt(52.55 - 2.75 * arc)}" rx=".24" ry=".34" fill="#f7f9f8" opacity="{_fmt(splash_opacity)}"/>
  <ellipse cx="{_fmt(53.15 + 1.45 * t)}" cy="{_fmt(52.58 - 2.25 * arc)}" rx=".2" ry=".29" fill="#f7f9f8" opacity="{_fmt(splash_opacity * .88)}"/>
  <ellipse cx="52.15" cy="52.88" rx="{_fmt(.7 + 3.7 * t)}" ry="{_fmt(.16 + .35 * t)}" fill="none" stroke="#d9e0df" stroke-width=".16" opacity="{_fmt((1.0 - t) * .56)}"/>"""
    else:
        # 착지 흔적은 사라지고 다음 방울이 정지 프레임의 자세까지 자라 루프를 잇는다.
        t = _smoothstep(_segment(u, 0.95, 1.0))
        pendant_x = 0.98 + 0.02 * t
        pendant_y = 0.96 + 0.04 * t
        if t > 0:
            drop = _falling_drop(
                top=39.75 + 1.25 * t,
                stretch=0.35 + 0.65 * t,
                width=0.36 + 0.64 * t,
                opacity=t,
                trail_top=39.82 + 0.36 * t,
                trail_opacity=0.36 * t,
            )

    upper = _upper_film()
    lower = _lower_film()
    liquid = f"""  <path fill="url(#b8-film)" d="{upper}"/>
  <path fill="url(#b8-film)" d="{lower}"/>
{_pendant_markup(pendant_x, pendant_y)}
{neck_residue}"""
    # 기존 두 흰 덩어리가 연결부를 덮도록 아래 덩어리의 명암 직후에 새 막을 얹는다.
    lower_shade = '  <path fill="#eef0ef" d="M39.6 37.2 C40.7 39 42 40.2 43.8 40.6 C42.6 39.4 42.5 37.4 43.4 36 C41.9 36.5 40.8 36.9 39.6 37.2Z"/>'
    svg = _insert_before(svg, lower_shade, liquid)

    if PUDDLE_PATH not in svg:
        raise ValueError("웅덩이 경로를 찾지 못함")
    if abs(puddle_x - 1.0) < 1e-6:
        puddle = PUDDLE_PATH
    else:
        puddle = (
            f'  <g transform="translate(51.1 0) scale({_fmt(puddle_x)} 1) translate(-51.1 0)">\n'
            f'{PUDDLE_PATH}\n  </g>'
        )
    ground = (
        f'  <ellipse cx="51.1" cy="55.55" rx="{_fmt(5.65 * puddle_x)}" '
        'ry="1.05" fill="#0d2625" opacity=".38"/>'
    )
    foreground = "\n".join(part for part in (ground, puddle, surface, drop) if part)
    svg = svg.replace(PUDDLE_PATH, foreground, 1)
    return svg


def _residue_markup(scale: float = 1.0) -> str:
    """낙하 뒤 막대에 남은 액체를 두 번째 방울이 아닌 작은 맺힘으로 그린다."""
    return (
        f'  <g transform="translate(53.05 35.58) scale({_fmt(scale)}) '
        'translate(-53.05 -35.58)">\n'
        '    <path fill="url(#b8-drop)" d="M52.53 35.56 '
        'C52.83 35.52 53.26 35.54 53.5 35.69 '
        'C53.73 35.84 53.69 36.16 53.48 36.3 '
        'C53.23 36.48 52.8 36.42 52.58 36.22 '
        'C52.4 36.05 52.36 35.69 52.53 35.56Z"/>\n'
        '    <path fill="#d9e0df" opacity=".72" d="M52.62 36.13 '
        'C52.84 36.34 53.2 36.4 53.45 36.25 '
        'C53.31 36.5 52.82 36.58 52.56 36.27Z"/>\n'
        '    <ellipse cx="52.72" cy="35.79" rx=".1" ry=".07" fill="#fff"/>\n'
        '  </g>'
    )


def _growing_drop_markup(t: float) -> str:
    """막대에 붙은 한 방울을 작은 맺힘에서 무거운 눈물형까지 키운다."""
    t = _clamp(t)
    center_x = 52.72
    neck = 0.17 + 0.13 * t
    # 96 px에서도 첫 맺힘이 한 픽셀보다 크게 남도록 최소 크기를 확보한다.
    radius_x = 0.4 + 0.72 * t
    radius_y = 0.36 + 1.09 * t
    bottom = 36.55 + 3.81 * t
    bulb_y = bottom - radius_y
    shoulder_y = 35.86 + 1.12 * t

    path = (
        f"M{_fmt(center_x - neck)} 35.56 "
        f"C{_fmt(center_x - neck)} {_fmt(35.79 + .3 * t)} "
        f"{_fmt(center_x - radius_x * .42)} {_fmt(shoulder_y)} "
        f"{_fmt(center_x - radius_x)} {_fmt(bulb_y - radius_y * .2)} "
        f"C{_fmt(center_x - radius_x * 1.03)} {_fmt(bulb_y + radius_y * .55)} "
        f"{_fmt(center_x - radius_x * .5)} {_fmt(bottom)} {_fmt(center_x)} {_fmt(bottom)} "
        f"C{_fmt(center_x + radius_x * .5)} {_fmt(bottom)} "
        f"{_fmt(center_x + radius_x * 1.03)} {_fmt(bulb_y + radius_y * .55)} "
        f"{_fmt(center_x + radius_x)} {_fmt(bulb_y - radius_y * .2)} "
        f"C{_fmt(center_x + radius_x * .42)} {_fmt(shoulder_y)} "
        f"{_fmt(center_x + neck)} {_fmt(35.79 + .3 * t)} "
        f"{_fmt(center_x + neck)} 35.56Z"
    )
    shade = (
        f"M{_fmt(center_x - radius_x * .82)} {_fmt(bulb_y + radius_y * .45)} "
        f"C{_fmt(center_x - radius_x * .45)} {_fmt(bottom - radius_y * .08)} "
        f"{_fmt(center_x + radius_x * .3)} {_fmt(bottom + .01)} "
        f"{_fmt(center_x + radius_x * .7)} {_fmt(bulb_y + radius_y * .48)} "
        f"C{_fmt(center_x + radius_x * .45)} {_fmt(bottom - radius_y * .16)} "
        f"{_fmt(center_x - radius_x * .5)} {_fmt(bottom - radius_y * .12)} "
        f"{_fmt(center_x - radius_x * .82)} {_fmt(bulb_y + radius_y * .45)}Z"
    )
    return f"""  <g>
    <path fill="url(#b8-drop)" d="{path}"/>
    <path fill="#0d2625" opacity=".12" d="M{_fmt(center_x - neck)} 35.58 C{_fmt(center_x - .2)} 35.52 {_fmt(center_x + .28)} 35.54 {_fmt(center_x + neck)} 35.68 C{_fmt(center_x + .18)} 35.78 {_fmt(center_x - .2)} 35.79 {_fmt(center_x - neck)} 35.68Z"/>
    <path fill="#d9e0df" opacity=".9" d="{shade}"/>
    <ellipse cx="{_fmt(center_x - radius_x * .32)}" cy="{_fmt(bulb_y - radius_y * .15)}" rx="{_fmt(.08 + .1 * t)}" ry="{_fmt(.06 + .07 * t)}" fill="#fff"/>
  </g>"""


def anim_frame(u: float) -> str:
    """시안2 전용 한 주기: 커짐 → 단일 방울 낙하 → 웅덩이 흡수."""
    if not 0 <= u < 1:
        raise ValueError("u는 0 이상 1 미만이어야 합니다")

    svg = _remove_main_dots(B6.read_text(encoding="utf-8"))
    defs = """    <linearGradient id="b8-film" gradientUnits="userSpaceOnUse" x1="45" y1="27" x2="52" y2="29">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset=".7" stop-color="#f6f8f7"/>
      <stop offset="1" stop-color="#d9e0df"/>
    </linearGradient>
    <radialGradient id="b8-drop" gradientUnits="userSpaceOnUse" cx="52.25" cy="37.1" r="3.7">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset=".7" stop-color="#f7f9f8"/>
      <stop offset="1" stop-color="#f3f6f5"/>
    </radialGradient>"""
    svg = _insert_before(svg, "  </defs>", defs)

    # 1.38 s 성장, 0.12 s 낙하, 0.10 s 흡수. 다음 주기의 작은 맺힘으로
    # 바로 이어지며 자유 낙하 방울·잔상·튀김·파문을 서로 겹치지 않는다.
    grow_end = 0.8625
    fall_end = 0.9375
    pendant = ""
    falling = ""
    surface = ""
    puddle_x = 1.0
    puddle_y = 1.0

    if u < grow_end:
        t = _smoothstep(u / grow_end)
        pendant = _growing_drop_markup(t)
    elif u < fall_end:
        t = _segment(u, grow_end, fall_end)
        # 8프레임에서는 이 구간이 한 장이므로 끊긴 직후보다 아래에 배치한다.
        top = 45.4 + 3.9 * t * t
        falling = _falling_drop(
            top=top,
            stretch=1.34 + 0.1 * t,
            width=0.94 - 0.04 * t,
        )
        pendant = _residue_markup(0.72)
        surface = (
            f'  <ellipse cx="52.35" cy="52.76" rx="{_fmt(1.18 - .43 * t)}" '
            f'ry="{_fmt(.25 - .08 * t)}" fill="#0d2625" '
            f'opacity="{_fmt(.12 + .24 * t)}"/>'
        )
    else:
        t = _segment(u, fall_end, 1.0)
        pulse = 1.0 - _smoothstep(t)
        puddle_x = 1.0 + 0.055 * pulse
        puddle_y = 1.0 + 0.025 * pulse
        pendant = _residue_markup(0.72 + 0.12 * _smoothstep(t))

    liquid = f"""  <path fill="url(#b8-film)" d="{_upper_film()}"/>
  <path fill="url(#b8-film)" d="{_lower_film()}"/>
{pendant}"""
    lower_shade = '  <path fill="#eef0ef" d="M39.6 37.2 C40.7 39 42 40.2 43.8 40.6 C42.6 39.4 42.5 37.4 43.4 36 C41.9 36.5 40.8 36.9 39.6 37.2Z"/>'
    svg = _insert_before(svg, lower_shade, liquid)

    if PUDDLE_PATH not in svg:
        raise ValueError("웅덩이 경로를 찾지 못함")
    if abs(puddle_x - 1.0) < 1e-6 and abs(puddle_y - 1.0) < 1e-6:
        puddle = PUDDLE_PATH
    else:
        puddle = (
            f'  <g transform="translate(51.1 54.2) scale({_fmt(puddle_x)} {_fmt(puddle_y)}) '
            f'translate(-51.1 -54.2)">\n{PUDDLE_PATH}\n  </g>'
        )
    ground = (
        f'  <ellipse cx="51.1" cy="55.55" rx="{_fmt(5.65 * puddle_x)}" '
        f'ry="{_fmt(1.05 * puddle_y)}" fill="#0d2625" opacity=".38"/>'
    )
    foreground = "\n".join(
        part for part in (ground, puddle, surface, falling) if part
    )
    return svg.replace(PUDDLE_PATH, foreground, 1)


def small_frame() -> str:
    """16~32 px용: 작은 자유 방울을 모두 빼고 낙하 눈물 한 개만 남긴다."""
    svg = B6_SMALL.read_text(encoding="utf-8")
    for cx, cy, radius in ((49, 19.8, 2.8), (49, 42, 3.1), (55, 45, 2.5)):
        pattern = (
            rf'\s*<circle cx="{cx:g}" cy="{cy:g}" r="{radius:g}" fill="#fff"/>'
        )
        svg, count = re.subn(pattern, "", svg, count=1)
        if count != 1:
            raise ValueError(f"소형판에서 지울 원을 찾지 못함: ({cx:g}, {cy:g})")

    upper = _upper_film(small=True)
    lower = _lower_film(small=True)
    # 소형판은 명암을 합치고 외곽을 한 번에 읽히게 한다.
    simple = f"""  <path fill="#fff" d="{upper}"/>
  <path fill="#fff" d="{lower}"/>
  <path fill="#fff" d="{_pendant()}"/>
  <path fill="#fff" d="M52.35 41.6 C52.27 42.1 51.48 42.73 51.45 43.52 C51.41 44.47 52.12 45.1 53 45.06 C53.99 45.02 54.48 44.18 54.2 43.36 C53.96 42.66 52.76 42.07 52.35 41.6Z"/>"""
    return _insert_before(svg, "</svg>", simple)


def make_zoom_preview(output: Path) -> None:
    """1600 px 렌더의 오른쪽 액체 줄기와 방울 구간을 지정 좌표로 자른다."""
    from render_logo import render, save_png

    raster = render(ASSETS / "logo_b8.svg", 1600)
    save_png(raster.copy(1000, 550, 600, 700), output)
    print(f"Preview: {output}")


def make_final_preview(output: Path) -> None:
    """확정 정지 로고를 512 px PNG로 렌더한다."""
    from render_logo import render, save_png

    save_png(render(ASSETS / "logo_b8.svg", 512), output)
    print(f"Preview: {output}")


def make_comparison_preview(output: Path) -> None:
    """QSvgRenderer 기반 6차안/8차안 512 px 나란히 비교표."""
    from PyQt6.QtCore import Qt
    from PyQt6.QtGui import QColor, QFont, QGuiApplication, QImage, QPainter

    from render_logo import render, save_png

    app = QGuiApplication.instance() or QGuiApplication([])
    assert app is not None
    canvas = QImage(1080, 584, QImage.Format.Format_ARGB32)
    canvas.fill(QColor("#ffffff"))
    painter = QPainter(canvas)
    font = QFont("Malgun Gothic")
    font.setPixelSize(18)
    painter.setFont(font)
    painter.setPen(QColor("#1e3736"))
    painter.drawText(20, 34, "6차안")
    painter.drawText(548, 34, "8차안")
    painter.drawImage(20, 52, render(B6, 512))
    painter.drawImage(548, 52, render(ASSETS / "logo_b8.svg", 512))
    painter.end()
    save_png(canvas, output)
    print(f"Preview: {output}")


def _render_svg(markup: str, size: int):
    """파일을 만들지 않고 SVG 문자열을 정사각 QImage로 렌더한다."""
    from PyQt6.QtCore import QByteArray, QRectF, Qt
    from PyQt6.QtGui import QImage, QPainter
    from PyQt6.QtSvg import QSvgRenderer

    renderer = QSvgRenderer(QByteArray(markup.encode("utf-8")))
    if not renderer.isValid():
        raise ValueError("frame(u)가 유효한 SVG를 만들지 못했습니다")
    image = QImage(size, size, QImage.Format.Format_ARGB32)
    image.fill(Qt.GlobalColor.transparent)
    painter = QPainter(image)
    renderer.render(painter, QRectF(0, 0, size, size))
    painter.end()
    return image


def _png_bytes(image) -> bytes:
    from PyQt6.QtCore import QByteArray, QBuffer, QIODevice

    payload = QByteArray()
    buffer = QBuffer(payload)
    buffer.open(QIODevice.OpenModeFlag.WriteOnly)
    if not image.save(buffer, "PNG"):
        raise OSError("프레임 PNG 인코딩 실패")
    buffer.close()
    return bytes(payload)


def _draw_bitmap_text(painter, x: int, y: int, value: str, color) -> None:
    """오프스크린 Qt에 글꼴이 없어도 검사표 숫자가 보이게 5×7로 그린다."""
    glyphs = {
        " ": ("00000",) * 7,
        ".": ("00000", "00000", "00000", "00000", "00000", "00110", "00110"),
        "=": ("00000", "00000", "11111", "00000", "11111", "00000", "00000"),
        "F": ("11111", "10000", "10000", "11110", "10000", "10000", "10000"),
        "u": ("00000", "00000", "10001", "10001", "10001", "10011", "01101"),
        "0": ("01110", "10001", "10011", "10101", "11001", "10001", "01110"),
        "1": ("00100", "01100", "00100", "00100", "00100", "00100", "01110"),
        "2": ("01110", "10001", "00001", "00010", "00100", "01000", "11111"),
        "3": ("11110", "00001", "00001", "01110", "00001", "00001", "11110"),
        "4": ("00010", "00110", "01010", "10010", "11111", "00010", "00010"),
        "5": ("11111", "10000", "10000", "11110", "00001", "00001", "11110"),
        "6": ("01110", "10000", "10000", "11110", "10001", "10001", "01110"),
        "7": ("11111", "00001", "00010", "00100", "01000", "01000", "01000"),
        "8": ("01110", "10001", "10001", "01110", "10001", "10001", "01110"),
        "9": ("01110", "10001", "10001", "01111", "00001", "00001", "01110"),
    }
    scale = 2
    painter.setBrush(color)
    painter.setPen(color)
    cursor = x
    for character in value:
        for row, pixels in enumerate(glyphs[character]):
            for column, pixel in enumerate(pixels):
                if pixel == "1":
                    painter.fillRect(
                        cursor + column * scale, y + row * scale, scale, scale, color
                    )
        cursor += 6 * scale


def make_draft_sheet(images: list, values: list[float], output: Path, *,
                     columns: int = 6, frame_size: int = 200) -> None:
    """오프스크린 렌더 프레임을 번호·u가 있는 검사표로 만든다."""
    from PyQt6.QtGui import QColor, QImage, QPainter

    if len(images) != len(values):
        raise ValueError("프레임 이미지 수와 u 값 수가 다릅니다")
    label_height = 28
    row_height = frame_size + label_height
    rows = math.ceil(len(images) / columns)
    sheet = QImage(columns * frame_size, rows * row_height, QImage.Format.Format_RGB32)
    sheet.fill(QColor("#102120"))
    painter = QPainter(sheet)
    for index, (image, u) in enumerate(zip(images, values)):
        x = index % columns * frame_size
        y = index // columns * row_height
        painter.drawImage(x, y, image)
        painter.fillRect(x, y + frame_size, frame_size, label_height, QColor("#162b2a"))
        label = f"F{index + 1:02d}  u={u:.4f}"
        label_x = 9
        if frame_size <= 160:
            label = label.replace("u=0.", "u=.")
            label_x = 5
        _draw_bitmap_text(
            painter, x + label_x, y + frame_size + 7,
            label, QColor("#d9e8e6"),
        )
        painter.setPen(QColor("#31504d"))
        painter.drawLine(x + frame_size - 1, y, x + frame_size - 1, y + row_height)
    painter.end()
    output.parent.mkdir(parents=True, exist_ok=True)
    if not sheet.save(str(output), "PNG"):
        raise OSError(f"프레임표 저장 실패: {output}")


def make_draft_html(images: list, values: list[float], output: Path, *,
                    durations_ms: list[float] | None = None,
                    period_ms: int = 1600,
                    heading: str = "흰 액체 낙하 · 12프레임 시안",
                    description: str = "SECS 로고 8차안 로딩 동작을 12프레임으로 검토하는 독립 미리보기입니다.",
                    note: str = "프레임은 PNG로 이 파일 안에 들어 있습니다. 멈춘 뒤 이전·다음으로 목 끊김, 낙하 잔상, 웅덩이 충돌을 한 장씩 확인할 수 있습니다.") -> None:
    """프레임을 가로 스프라이트 하나로 합쳐 내장한 단일 파일 재생 검토대를 만든다."""
    from PyQt6.QtCore import Qt
    from PyQt6.QtGui import QImage, QPainter

    if not images or len(images) != len(values):
        raise ValueError("프레임 이미지와 u 값이 같은 수로 있어야 합니다")
    frame_width = images[0].width()
    frame_height = images[0].height()
    if any(image.width() != frame_width or image.height() != frame_height for image in images):
        raise ValueError("스프라이트 프레임 크기가 서로 다릅니다")
    sprite = QImage(frame_width * len(images), frame_height, QImage.Format.Format_ARGB32)
    sprite.fill(Qt.GlobalColor.transparent)
    painter = QPainter(sprite)
    for index, image in enumerate(images):
        painter.drawImage(index * frame_width, 0, image)
    painter.end()
    sprite_url = "data:image/png;base64," + base64.b64encode(_png_bytes(sprite)).decode("ascii")
    if durations_ms is None:
        durations_ms = []
        for index, value in enumerate(values):
            following = values[(index + 1) % len(values)]
            span = following - value if following > value else 1 - value + following
            durations_ms.append(period_ms * span)
    if len(durations_ms) != len(values) or not math.isclose(sum(durations_ms), period_ms):
        raise ValueError("프레임 머무는 시간은 프레임 수와 같고 합계가 주기여야 합니다")
    template = r'''<!doctype html>
<html lang="ko">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="description" content="__DESCRIPTION__">
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='7' fill='%231e3736'/%3E%3Ccircle cx='16' cy='16' r='5' fill='%2330e3da'/%3E%3C/svg%3E">
<title>SECS · __HEADING__</title>
<style>
/* 계측 장비의 섀시(#1e3736), 신호 표시등(#30e3da), 응답 곡선(#ff0a6c)을 그대로 쓴다. */
:root{--chassis:#1e3736;--deep:#0b1717;--rack:#102322;--line:#31504d;--ink:#e8f1f0;--muted:#8ea9a6;--marker:#30e3da;--trace:#ff0a6c;--frame:256px;--ui:'Segoe UI','IBM Plex Sans KR','Malgun Gothic',sans-serif;--mono:'IBM Plex Mono','D2Coding','Malgun Gothic',monospace}
*{box-sizing:border-box}
html{color-scheme:dark}
body{margin:0;min-height:100vh;background:var(--deep);color:var(--ink);font-family:var(--ui);word-break:keep-all;overflow-wrap:break-word}
button{font:inherit}
button:focus-visible{outline:2px solid var(--marker);outline-offset:3px}
.bench{width:min(980px,calc(100% - 32px));margin:0 auto;padding:34px 0 42px}
.mast{display:flex;align-items:end;justify-content:space-between;gap:28px;border-bottom:1px solid var(--line);padding-bottom:15px}
.eyebrow{margin:0 0 4px;color:var(--marker);font:600 11px/1.3 var(--mono);letter-spacing:.12em}
h1{margin:0;font-size:clamp(22px,4vw,36px);font-weight:650;letter-spacing:-.025em;text-wrap:balance}
.spec{display:grid;grid-template-columns:auto auto;gap:2px 18px;margin:0;color:var(--muted);font:12px/1.5 var(--mono);font-variant-numeric:tabular-nums}
.spec dt,.spec dd{margin:0}.spec dd{color:var(--ink);text-align:right}
.stage{display:grid;grid-template-columns:minmax(270px,1fr) minmax(360px,1.42fr);gap:1px;margin-top:18px;background:var(--line);border:1px solid var(--line)}
.scope{position:relative;display:grid;place-items:center;min-height:358px;background-color:var(--rack);background-image:linear-gradient(rgba(48,227,218,.035) 1px,transparent 1px),linear-gradient(90deg,rgba(48,227,218,.035) 1px,transparent 1px);background-size:16px 16px;overflow:hidden}
.scope::before{content:attr(data-size);position:absolute;top:12px;left:14px;color:var(--muted);font:11px/1 var(--mono);letter-spacing:.08em}
.scope::after{content:"";position:absolute;inset:8px;border:1px solid rgba(142,169,166,.16);pointer-events:none}
.scope .sprite{display:block;width:var(--display);height:var(--display);background-repeat:no-repeat;image-rendering:auto}
.scope.small{--display:96px}.scope.large{--display:256px}
.readout{display:flex;align-items:center;justify-content:space-between;gap:18px;margin-top:1px;padding:12px 14px;background:#132827;border:1px solid var(--line);font:12px/1.4 var(--mono);font-variant-numeric:tabular-nums}
.pulse{display:inline-block;width:7px;height:7px;margin-right:8px;background:var(--marker);box-shadow:0 0 9px rgba(48,227,218,.45)}
.timeline{height:2px;flex:1;background:#294543;overflow:hidden}.timeline i{display:block;width:0;height:100%;background:var(--trace)}
.controls{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-top:12px}
.controls button{min-height:38px;border:1px solid #42615e;border-radius:2px;background:#142a29;color:var(--ink);padding:7px 13px;cursor:pointer}
.controls button:hover{border-color:#6c8d89;background:#193230}.controls button[aria-pressed="true"]{border-color:var(--marker);color:var(--marker);background:#102726}
.controls .transport{border-color:#647a78;background:#dfe9e8;color:#142625;font-weight:650}
.controls .transport:hover{background:#fff;border-color:#fff}
.divider{width:1px;height:28px;background:var(--line);margin:0 2px}
.note{max-width:690px;margin:16px 0 0;color:var(--muted);font-size:12px;line-height:1.65}
@media(max-width:760px){.bench{width:min(calc(100% - 20px),620px);padding-top:22px}.mast{align-items:start;flex-direction:column;gap:12px}.stage{grid-template-columns:1fr}.scope{min-height:304px}.scope.small{min-height:190px}.spec{width:100%}}
@media(max-width:390px){.controls{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr)}.controls .divider{display:none}.controls button{min-width:0;padding-inline:7px;font-size:12px}.scope.large{--display:min(256px,78vw)}}
@media(prefers-reduced-motion:reduce){.pulse{box-shadow:none}}
</style>
</head>
<body>
<main class="bench">
  <header class="mast">
    <div><p class="eyebrow">SECS / MOTION CHECK B8</p><h1>__HEADING__</h1></div>
    <dl class="spec"><dt>주기</dt><dd>__PERIOD_SECONDS__ s</dd><dt>프레임</dt><dd>__FRAME_COUNT__</dd><dt>표시</dt><dd>96 / 256 px</dd></dl>
  </header>
  <section class="stage" aria-label="애니메이션 크기 비교">
    <div class="scope small" data-size="96 PX"><div class="sprite" id="preview96" role="img" aria-label="96픽셀 로딩 로고"></div></div>
    <div class="scope large" data-size="256 PX"><div class="sprite" id="preview256" role="img" aria-label="256픽셀 로딩 로고"></div></div>
  </section>
  <div class="readout" aria-live="polite"><span><i class="pulse"></i><span id="status">준비</span></span><span class="timeline" aria-hidden="true"><i id="progress"></i></span></div>
  <nav class="controls" aria-label="재생 제어">
    <button class="transport" id="play" type="button">멈춤</button>
    <button id="previous" type="button">← 이전 프레임</button>
    <button id="next" type="button">다음 프레임 →</button>
    <span class="divider" aria-hidden="true"></span>
    <button class="speed" data-speed="1" aria-pressed="true" type="button">1×</button>
    <button class="speed" data-speed="0.25" aria-pressed="false" type="button">0.25×</button>
  </nav>
  <p class="note">__NOTE__</p>
</main>
<script>
const SPRITE=__SPRITE__;
const FRAME_COUNT=__FRAME_COUNT__;
const U=__VALUES__;
const DWELL=__DURATIONS__;
const PERIOD=__PERIOD_MS__;
const view96=document.getElementById('preview96');
const view256=document.getElementById('preview256');
const status=document.getElementById('status');
const progress=document.getElementById('progress');
const play=document.getElementById('play');
let index=0,speed=1,running=true,nextAt=0;
function frameDuration(i){return DWELL[i]/speed}
function paintSprite(view,size){view.style.backgroundImage=`url(${SPRITE})`;view.style.backgroundSize=`${FRAME_COUNT*size}px ${size}px`;view.style.backgroundPosition=`-${index*size}px 0`}
function show(){paintSprite(view96,96);paintSprite(view256,256);status.textContent=`F${String(index+1).padStart(2,'0')} / ${FRAME_COUNT} · u=${U[index].toFixed(4)} · ${DWELL[index].toFixed(1)} ms · ${speed}× · ${running?'재생':'멈춤'}`;progress.style.width=`${(index+1)/FRAME_COUNT*100}%`;play.textContent=running?'멈춤':'재생'}
function setRunning(value){running=value;nextAt=performance.now()+frameDuration(index);show()}
function step(delta){running=false;index=(index+delta+FRAME_COUNT)%FRAME_COUNT;show()}
function setSpeed(value){speed=value;document.querySelectorAll('.speed').forEach(button=>button.setAttribute('aria-pressed',Number(button.dataset.speed)===speed?'true':'false'));nextAt=performance.now()+frameDuration(index);show()}
function tick(now){if(running&&now>=nextAt){do{index=(index+1)%FRAME_COUNT;nextAt+=frameDuration(index)}while(now>=nextAt);show()}requestAnimationFrame(tick)}
play.addEventListener('click',()=>setRunning(!running));
document.getElementById('previous').addEventListener('click',()=>step(-1));
document.getElementById('next').addEventListener('click',()=>step(1));
document.querySelectorAll('.speed').forEach(button=>button.addEventListener('click',()=>setSpeed(Number(button.dataset.speed))));
if(window.matchMedia('(prefers-reduced-motion: reduce)').matches)running=false;
nextAt=performance.now()+frameDuration(index);show();requestAnimationFrame(tick);
</script>
</body>
</html>
'''
    markup = template.replace("__SPRITE__", json.dumps(sprite_url))
    markup = markup.replace("__VALUES__", json.dumps(values))
    markup = markup.replace("__DURATIONS__", json.dumps(durations_ms))
    markup = markup.replace("__PERIOD_MS__", str(period_ms))
    markup = markup.replace("__PERIOD_SECONDS__", f"{period_ms / 1000:.3f}")
    markup = markup.replace("__DESCRIPTION__", description)
    markup = markup.replace("__HEADING__", heading)
    markup = markup.replace("__FRAME_COUNT__", str(len(values)))
    markup = markup.replace("__NOTE__", note)
    output.write_text(markup, encoding="utf-8", newline="\n")


def make_draft12() -> None:
    """각 동작 구간을 포함하는 12프레임 시안과 자체 포함 HTML만 만든다."""
    # 0.45·0.55·0.80·0.95의 짧은 경계를 12장에서도 읽을 수 있게 핵심 시각을
    # 골라 담는다. HTML은 u 간격만큼 각 장을 유지하므로 전체 주기는 1.6초다.
    values = [0.0, 0.12, 0.24, 0.36, 0.46, 0.52,
              0.57, 0.66, 0.75, 0.82, 0.90, 0.975]
    table_images = [_render_svg(frame(u), 200) for u in values]
    html_images = [_render_svg(frame(u), 256) for u in values]
    REPORT.mkdir(parents=True, exist_ok=True)
    table = REPORT / "시안_12프레임_표.png"
    preview = REPORT / "시안_12프레임.html"
    make_draft_sheet(table_images, values, table)
    make_draft_html(html_images, values, preview)
    print(f"Draft sheet: {table} (12 frames, 200 px, 6x2)")
    print(f"Draft HTML: {preview} (12 embedded PNG frames, 256 px)")


def make_draft8_v2() -> None:
    """커짐 6장, 낙하 1장, 흡수 1장의 시안2만 만든다."""
    values = [0.0, 0.15625, 0.3125, 0.4625,
              0.60625, 0.74375, 0.8625, 0.9375]
    durations_ms = [250, 250, 240, 230, 220, 190, 120, 100]
    table_images = [_render_svg(anim_frame(u), 240) for u in values]
    html_images = [_render_svg(anim_frame(u), 256) for u in values]
    REPORT.mkdir(parents=True, exist_ok=True)
    table = REPORT / "시안2_8프레임_표.png"
    preview = REPORT / "시안2_8프레임.html"
    make_draft_sheet(table_images, values, table, columns=4, frame_size=240)
    make_draft_html(
        html_images,
        values,
        preview,
        durations_ms=durations_ms,
        heading="커지다 뚝 떨어짐 · 8프레임 시안2",
        description="SECS 로고 8차안의 방울이 커지고 떨어져 흡수되는 8프레임 시안2입니다.",
        note="F01~F06은 방울이 단계적으로 커지고, F07에서 한 방울만 떨어지며, F08에서 웅덩이에 흡수됩니다. 멈춘 뒤 이전·다음으로 한 장씩 확인할 수 있습니다.",
    )
    print(f"Draft sheet: {table} (8 frames, 240 px, 4x2)")
    print(f"Draft HTML: {preview} (8 embedded PNG frames, 256 px)")
    print("Timing: " + ", ".join(
        f"F{index + 1:02d} u={u:.5f} {duration}ms"
        for index, (u, duration) in enumerate(zip(values, durations_ms))
    ))


def make_frames() -> None:
    """시안2를 같은 간격의 24장으로 렌더해 앱 스프라이트와 확정 미리보기를 만든다."""
    from PyQt6.QtCore import Qt
    from PyQt6.QtGui import QImage, QPainter

    frame_count = 24
    frame_size = 320
    period_ms = 1067
    # 시안2의 누적 머무름은 u 증가량과 정확히 비례한다. 따라서 같은 시간 간격의
    # 24장에서는 u도 1/24 간격이며, 낙하 구간(0.8625~0.9375)은 두 장으로 남는다.
    values = [index / frame_count for index in range(frame_count)]
    frames = [_render_svg(anim_frame(u), frame_size) for u in values]
    sprite = QImage(frame_size * frame_count, frame_size, QImage.Format.Format_ARGB32)
    sprite.fill(Qt.GlobalColor.transparent)
    painter = QPainter(sprite)
    for index, image in enumerate(frames):
        painter.drawImage(index * frame_size, 0, image)
    painter.end()

    ASSETS.mkdir(parents=True, exist_ok=True)
    sprite_path = ASSETS / "loading_drip.png"
    if not sprite.save(str(sprite_path), "PNG"):
        raise OSError(f"스프라이트 저장 실패: {sprite_path}")
    if sprite_path.stat().st_size > 3 * 1024 * 1024:
        raise ValueError(f"스프라이트가 3 MB를 넘습니다: {sprite_path.stat().st_size} bytes")

    REPORT.mkdir(parents=True, exist_ok=True)
    table_path = REPORT / "확정_24프레임_표.png"
    html_path = REPORT / "확정_24프레임.html"
    table_images = [_render_svg(anim_frame(u), 160) for u in values]
    make_draft_sheet(table_images, values, table_path, columns=6, frame_size=160)
    make_draft_html(
        frames,
        values,
        html_path,
        durations_ms=[period_ms / frame_count] * frame_count,
        period_ms=period_ms,
        heading="커지다 뚝 떨어짐 · 확정 24프레임",
        description="SECS 로고 방울의 확정 24프레임 로딩 애니메이션을 실제 앱 스프라이트로 재생합니다.",
        note="앱에 들어가는 192 px 스프라이트 한 장을 잘라 재생합니다. 1×는 1.067초 주기이며, 멈춘 뒤 이전·다음으로 낙하 두 장과 흡수 장면을 확인할 수 있습니다.",
    )
    print(f"Sprite: {sprite_path} ({sprite.width()}x{sprite.height()}, {sprite_path.stat().st_size} bytes)")
    print(f"Final sheet: {table_path} (24 frames, 160 px, 6x4)")
    print(f"Final HTML: {html_path} (sprite playback, {period_ms} ms)")
    print("u: " + ", ".join(f"{u:.6f}" for u in values))


def make_static_assets() -> None:
    """기존 확정 SVG와 정지 미리보기를 재생성한다."""

    outputs = (
        (ASSETS / "logo_b8.svg", frame(0.0)),
        (ASSETS / "logo_b8_small.svg", small_frame()),
    )
    for path, markup in outputs:
        path.write_text(markup, encoding="utf-8")
        print(f"{path} - {len(markup.encode('utf-8'))} bytes")
    REPORT.mkdir(parents=True, exist_ok=True)
    make_final_preview(REPORT / "b8_확정_512.png")
    make_comparison_preview(REPORT / "b6_vs_b8_512.png")
    from render_logo import make_size_preview

    make_size_preview(ASSETS / "logo_b8.svg", ASSETS / "logo_b8_small.svg",
                      REPORT / "b8_크기별.png")
    make_zoom_preview(REPORT / "b8_확대.png")


def main() -> None:
    from PyQt6.QtGui import QGuiApplication

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--draft12",
        action="store_true",
        help="앱 assets는 건드리지 않고 12프레임 저해상도 시안만 만든다",
    )
    parser.add_argument(
        "--draft8-v2",
        action="store_true",
        help="앱 assets는 건드리지 않고 시안2 8프레임만 만든다",
    )
    parser.add_argument(
        "--frames",
        action="store_true",
        help="시안2 확정 24프레임 스프라이트와 미리보기를 만든다",
    )
    args = parser.parse_args()
    app = QGuiApplication.instance() or QGuiApplication([])
    assert app is not None
    if args.frames:
        make_frames()
    elif args.draft8_v2:
        make_draft8_v2()
    elif args.draft12:
        make_draft12()
    else:
        make_static_assets()


if __name__ == "__main__":
    main()
