"""估算模板在 1280x720 基准下的画布显示尺寸（ncc_match 的 size 参数）。

用法：
    python ncc_helper.py <截图> <模板> [x y w h]

示例：
    python ncc_helper.py ease.png T_AngLishFish_Auto.png
    python ncc_helper.py shot.png track_point.png 560 220 160 160

ROI 为 1280x720 基准坐标，默认0 0 0 0 表示整张截图。

评分与引擎保持一致：亮度 + 色度的 RGB 零均值互相关，色度能量占比 >= 5% 时
再乘色度约束。因此彩色模板（如 track_point）按 RGB 匹配，不会只靠灰度。
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image

BASE_W, BASE_H = 1280, 720
DEFAULT_ROI = (0, 0, 0, 0)
COLOR_GATE = 0.05

def load_template(path: Path) -> tuple[np.ndarray, int, int]:
    """返回 (RGB 信号, 源图宽, 源图高)。

    alpha 贴图先按 alpha 预乘保留颜色；若 RGB 为空（只有遮罩），退回 alpha 灰度。
    """
    image = Image.open(path)
    rgba = np.asarray(image.convert("RGBA")).astype(np.float64)
    alpha = rgba[:, :, 3]
    if alpha.min() < 250:
        rgb = rgba[:, :, :3] * (alpha[:, :, None] / 255.0)
        if np.ptp(rgb) < 1e-6:
            rgb = np.repeat(alpha[:, :, None], 3, axis=2)
        ys, xs = np.where(alpha > 0)
        if len(xs) == 0:
            sys.exit(f"模板内容为空：{path}")
        rgb = rgb[ys.min():ys.max() + 1, xs.min():xs.max() + 1]
    else:
        rgb = rgba[:, :, :3]
    return rgb, image.width, image.height

def features(rgb: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """与 src/vision.rs 的 ncc_features 相同的正交分解。"""
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    luma = (r + g + b) * 0.5773502691896258
    c1 = (r - g) * 0.7071067811865476
    c2 = (r + g - 2.0 * b) * 0.4082482904638631
    return luma, c1, c2

def integral(array: np.ndarray) -> np.ndarray:
    return np.pad(array.cumsum(0).cumsum(1), ((1, 0), (1, 0)))

def window_sum(table: np.ndarray, th: int, tw: int) -> np.ndarray:
    return table[th:, tw:] - table[:-th, tw:] - table[th:, :-tw] + table[:-th, :-tw]

def correlate(spectrum: np.ndarray, template: np.ndarray, shape: tuple[int, int], out_shape: tuple[int, int]) -> np.ndarray:
    th, tw = template.shape
    flipped = np.fft.rfft2(template[::-1, ::-1], shape)
    full = np.fft.irfft2(spectrum * flipped, shape)
    return full[th - 1:out_shape[0], tw - 1:out_shape[1]]

if len(sys.argv) < 3:
    sys.exit(__doc__)

shot_path = Path(sys.argv[1])
tpl_path = Path(sys.argv[2])
if not shot_path.exists():
    sys.exit(f"找不到截图：{shot_path}")
if not tpl_path.exists():
    sys.exit(f"找不到模板：{tpl_path}")
roi = tuple(map(int, sys.argv[3:7])) if len(sys.argv) >= 7 else DEFAULT_ROI

shot = np.asarray(Image.open(shot_path).convert("RGB")).astype(np.float64)
shot_h, shot_w = shot.shape[:2]
scale_x, scale_y = shot_w / BASE_W, shot_h / BASE_H

if roi[2] > 0 and roi[3] > 0:
    x0 = round(roi[0] * scale_x)
    y0 = round(roi[1] * scale_y)
    rw = round(roi[2] * scale_x)
    rh = round(roi[3] * scale_y)
    region = shot[y0:y0 + rh, x0:x0 + rw]
else:
    x0 = y0 = 0
    rw, rh = shot_w, shot_h
    region = shot

signal, src_w, src_h = load_template(tpl_path)
max_canvas = min(rw, rh, 512)
shape = (rh + max_canvas, rw + max_canvas)

luma, c1, c2 = features(region)
spectrum_luma = np.fft.rfft2(luma, shape)
spectrum_c1 = np.fft.rfft2(c1, shape)
spectrum_c2 = np.fft.rfft2(c2, shape)
integral_luma = integral(luma)
integral_luma_sq = integral(luma * luma)
integral_chroma_sq = integral(c1 * c1 + c2 * c2)
out_shape = (rh, rw)

best_score, best_size, best_pos = -2.0, 0, (0, 0)
for canvas in range(16, max_canvas + 1):
    tw = max(4, round(signal.shape[1] * canvas / src_w))
    th = max(4, round(signal.shape[0] * canvas / src_h))
    if th > rh or tw > rw:
        continue
    template = np.asarray(
        Image.fromarray(np.clip(signal, 0, 255).astype(np.uint8)).resize(
            (tw, th), Image.Resampling.LANCZOS
        ),
        np.float64,
    )
    tpl_luma, tpl_c1, tpl_c2 = features(template)
    tpl_luma -= tpl_luma.mean()
    luma_energy = float((tpl_luma ** 2).sum())
    color_energy = float((tpl_c1 ** 2 + tpl_c2 ** 2).sum())
    total_energy = luma_energy + color_energy
    if total_energy <= 1e-8:
        continue
    has_color = color_energy / total_energy >= COLOR_GATE
    template_energy = total_energy if has_color else luma_energy

    samples = tw * th
    ld = correlate(spectrum_luma, tpl_luma, shape, out_shape)
    luma_sum = window_sum(integral_luma, th, tw)
    le = np.maximum(window_sum(integral_luma_sq, th, tw) - luma_sum * luma_sum / samples, 0.0)
    if has_color:
        ce = window_sum(integral_chroma_sq, th, tw)
        cd = correlate(spectrum_c1, tpl_c1, shape, out_shape) + correlate(spectrum_c2, tpl_c2, shape, out_shape)
    else:
        ce = np.zeros_like(le)
        cd = np.zeros_like(le)

    energy = le + ce
    with np.errstate(invalid="ignore", divide="ignore"):
        score = np.where(energy > 1e-8, (ld + cd) / np.sqrt(energy * template_energy), 0.0)
    score = np.clip(score, -1.0, 1.0)
    if has_color:
        gate = np.where(
            ce > 1e-8,
            np.clip(cd / np.sqrt(np.maximum(ce, 1e-12) * color_energy), 0.0, 1.0),
            0.0,
        )
        score = score * gate

    j, i = np.unravel_index(np.argmax(score), score.shape)
    value = float(score[j, i])
    if value > best_score:
        best_score = value
        best_size = canvas
        best_pos = (x0 + i + tw // 2, y0 + j + th // 2)

cx = round(best_pos[0] / scale_x)
cy = round(best_pos[1] / scale_y)
size = best_size * BASE_H / shot_h
print(f"模板：{tpl_path}")
print(f"截图：{shot_path} ({shot_w}x{shot_h})")
print(f"画布：{size:.0f}  (1280x720 基准，截图内 {best_size}px)")
print(f"中心：({cx}, {cy})  得分：{best_score:.3f}")
