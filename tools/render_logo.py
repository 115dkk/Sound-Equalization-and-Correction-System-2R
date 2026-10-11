"""로고 SVG를 미리보기 PNG 또는 앱 ICO/PNG로 렌더하는 독립 도구.

--ico SVG로 실행하면 7개 크기를 담은 app.ico와 app_256.png를 만든다
(인자 없이 돌리던 09-20 후보 A·B·C 비교표 모드는 후보 SVG 와 함께 2026-10-10 에 지웠다). --small SVG는 16~32px에
사용하고, --preview PNG는 선택한 SVG의 크기 비교표만 만든다. 제품 모듈은 불러오지 않는다.

디자인 근거(2026-09-20): 보정 커튼, 측정 응답, 평탄한 타깃, 저역 통합 경로,
8단계 진행 마커가 이 제품의 다섯 실물이다. A는 커튼 안의 평탄한 구간,
B는 측정/보정 전후, C는 두 경로의 합류를 표현한다. 팔레트는 style.COLORS의
chassis/marker/white에서 출발한다. B3는 사용자 손그림 2의 연어색 띠,
붉은 접점, 흰 액체를 반영하며 Qt 지원 radialGradient만 사용한다.
심벌은 무문자이며 프리뷰 글꼴은 Windows의 한글 판독용 맑은 고딕이다.
"""
from __future__ import annotations

import argparse
import struct
from pathlib import Path

from PyQt6.QtCore import QByteArray, QBuffer, QIODevice, QRectF, Qt
from PyQt6.QtGui import QColor, QFont, QGuiApplication, QImage, QPainter
from PyQt6.QtSvg import QSvgRenderer

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "src/secs/wizard/assets"
SIZES = (16, 24, 32, 48, 64, 128, 256)
CHASSIS, MARKER, WHITE = "#1e3736", "#30e3da", "#ffffff"


def render(svg: Path, size: int) -> QImage:
    renderer = QSvgRenderer(str(svg))
    if not renderer.isValid():
        raise ValueError(f"유효하지 않은 SVG: {svg}")
    result = QImage(size, size, QImage.Format.Format_ARGB32)
    result.fill(Qt.GlobalColor.transparent)
    painter = QPainter(result)
    renderer.render(painter, QRectF(0, 0, size, size))
    painter.end()
    return result


def save_png(img: QImage, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not img.save(str(path), "PNG"):
        raise OSError(f"PNG 저장 실패: {path}")


def png_bytes(img: QImage) -> bytes:
    data = QByteArray()
    buffer = QBuffer(data)
    buffer.open(QIODevice.OpenModeFlag.WriteOnly)
    if not img.save(buffer, "PNG"):
        raise OSError("ICO 내부 PNG 인코딩 실패")
    buffer.close()
    return bytes(data)


def make_ico(svg: Path, small: Path | None = None) -> None:
    # 각 크기를 SVG에서 직접 렌더해 16px 직선이 축소 필터로 번지지 않게 한다.
    frames = {size: render(small if small and size <= 32 else svg, size)
              for size in SIZES}
    ASSETS.mkdir(parents=True, exist_ok=True)
    save_png(frames[256], ASSETS / "app_256.png")
    try:
        from PIL import Image
    except ImportError:
        payloads = [png_bytes(frames[size]) for size in SIZES]
        offset = 6 + 16 * len(SIZES)
        directory = bytearray(struct.pack("<HHH", 0, 1, len(SIZES)))
        for size, payload in zip(SIZES, payloads):
            directory.extend(struct.pack("<BBBBHHII", size % 256, size % 256,
                                         0, 0, 1, 32, len(payload), offset))
            offset += len(payload)
        (ASSETS / "app.ico").write_bytes(bytes(directory) + b"".join(payloads))
    else:
        from io import BytesIO
        images = [Image.open(BytesIO(png_bytes(frames[size]))).convert("RGBA")
                  for size in SIZES]
        images[-1].save(ASSETS / "app.ico", format="ICO",
                        sizes=[(size, size) for size in SIZES],
                        append_images=images[:-1])
    print(f"ICO: {ASSETS / 'app.ico'} ({', '.join(map(str, SIZES))} px)")
    print(f"PNG: {ASSETS / 'app_256.png'}")


def make_size_preview(svg: Path, small: Path | None, output: Path) -> None:
    """본판과 소형판을 실제 크기 및 최근접 4배 확대로 두 배경에서 비교한다."""
    sheet = QImage(760, 772, QImage.Format.Format_ARGB32)
    sheet.fill(QColor(WHITE))
    painter = QPainter(sheet)
    font = QFont("Malgun Gothic")
    font.setPixelSize(15)
    painter.setFont(font)
    painter.setPen(QColor(CHASSIS))
    painter.drawText(24, 30, "로고 8차안 확정 · 2026-09-24 / 16~32 px 전용 단순판")
    for row, (bg, fg) in enumerate(((CHASSIS, WHITE), (WHITE, CHASSIS))):
        top = 48 + row * 362
        painter.fillRect(0, top, sheet.width(), 362, QColor(bg))
        painter.setPen(QColor(fg))
        painter.drawText(24, top + 27, "어두운 배경" if row == 0 else "밝은 배경")
        for x, size, enlarged in ((24, 256, False), (324, 64, False),
                                  (438, 32, False), (530, 16, False),
                                  (626, 16, True)):
            raster = render(small if small and size <= 32 else svg, size)
            if enlarged:
                raster = raster.scaled(64, 64, Qt.AspectRatioMode.IgnoreAspectRatio,
                                       Qt.TransformationMode.FastTransformation)
            painter.drawImage(x, top + 52 + (256 - raster.height()) // 2, raster)
            painter.drawText(x, top + 337, "16 px ×4" if enlarged else f"{size} px")
    painter.end()
    save_png(sheet, output)
    print(f"Preview: {output}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ico", type=Path, metavar="SVG", help="앱 아이콘으로 쓸 SVG 경로")
    parser.add_argument("--small", type=Path, metavar="SVG", help="16·24·32 px 전용 SVG")
    parser.add_argument("--preview", type=Path, metavar="PNG", help="--ico SVG의 비교표만 저장")
    args = parser.parse_args()
    if not args.ico:
        parser.error("--ico SVG 가 필요합니다(--small · --preview 는 --ico 와 함께)")
    app = QGuiApplication.instance() or QGuiApplication([])
    svg = args.ico.resolve()
    small = args.small.resolve() if args.small else None
    if args.preview:
        make_size_preview(svg, small, args.preview.resolve())
    else:
        make_ico(svg, small)


if __name__ == "__main__":
    main()
