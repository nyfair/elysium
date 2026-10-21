#!/usr/bin/env python
import argparse
import math
from dataclasses import dataclass, field, replace
from itertools import groupby
from pathlib import Path

# ============================================================ 游戏常量

TUNING = (64, 59, 55, 50, 45, 40)   # 1–6 弦空弦：E4 B3 G3 D3 A2 E2
MAX_FRET = 24
PAGE_SIZE = 6
OPEN_PAGE = 5
START_PAGE = 1

DIVISION = 480                      # 输出时间轴：480 tick / 拍
DEFAULT_TEMPO_US = 500_000
STICK_MAX = 32767

STRING_BUTTONS = {6: "X", 5: "A", 4: "DPAD_UP", 3: "DPAD_RIGHT", 2: "DPAD_LEFT", 1: "DPAD_DOWN"}

LEAD_IN = 0.500                     # 开场留白：切模式 + 初始翻页
AIM_GAP = 0.065                     # 摇杆到位 -> 按弦；50ms 仍会弹成上一个品
REPLUCK_GAP = 0.040                 # 同弦重拨前的松开提前量
MIN_PRESS = 0.005
TAP_SECONDS = 0.080                 # 不重叠的单音点按时长
PAGE_TAP_HOLD = 0.040               # 翻页键按住时长
PAGE_TAP_GAP = 0.030                # 两次翻页之间
PAGE_LEAD = 0.080                   # 翻页要在音符前完成

STRUM_DOWN = (0, -STICK_MAX)
STRUM_UP = (0, STICK_MAX)
CHORD_STRUM_FLICK = 0.050           # 右摇杆推住多久再回中

# ============================================================ 和弦表

# 顺序 = 游戏里的位置：第 1–6 个是第 1 页六个方向，7–12 第 2 页，依此类推。
# 指法 6 项按 1 弦 -> 6 弦，-1 = 不发声。
CHORDS: tuple[tuple[str, tuple[int, ...]], ...] = (
    ("G", (3, 2, 0, 0, 0, 3)),
    ("Am", (0, 0, 2, 2, 1, 0)),
    ("Bm", (2, 2, 4, 4, 3, 2)),
    ("C", (0, 3, 2, 0, 1, 0)),
    ("D", (2, 3, 2, 0, -1, -1)),
    ("Em", (0, 2, 2, 0, 0, 0)),
    ("Cmaj7", (0, 3, 2, 0, 0, 0)),
    ("C7", (0, 3, 2, 3, 1, 0)),
    ("Cm7", (3, 3, 5, 3, 4, 3)),
    ("G7", (3, 2, 0, 0, 0, 1)),
    ("C9", (0, 3, 2, 3, 3, 0)),
    ("Cm9", (3, 3, 5, 3, 4, 3)),
    ("C5", (0, 3, 2, 0, -1, -1)),
    ("D5", (2, 3, 0, 0, -1, -1)),
    ("G5", (3, 2, 0, 0, -1, -1)),
    ("A5", (0, 0, 2, 2, -1, -1)),
    ("Cmaj9", (0, 3, 2, 0, 3, 0)),
    ("Cadd9", (0, 3, 2, 0, 3, 0)),
    ("G/B", (0, 2, 0, 0, 0, 3)),
    ("Am7/G", (0, 0, 2, 0, 1, 0)),
    ("D/F#", (2, 3, 2, 0, 2, -1)),
    ("F/A", (0, 0, 3, 2, 1, 1)),
    ("A", (0, 0, 2, 2, 2, 0)),
    ("Dm", (1, 3, 2, 0, -1, -1)),
)

CHORD_INDEX = {name: i for i, (name, _frets) in enumerate(CHORDS)}

# 游戏里能弹的大三/小三和弦（F 大调只有 F/A 这个转位）。
GAME_CHORDS = (
    ("C", 0, False), ("D", 2, False), ("G", 7, False), ("A", 9, False), ("F/A", 5, False),
    ("Am", 9, True), ("Bm", 11, True), ("Em", 4, True), ("Dm", 2, True),
)
GAME_PCS = {
    name: {root, (root + (3 if minor else 4)) % 12, (root + 7) % 12}
    for name, root, minor in GAME_CHORDS
}
GAME_PAGE = {name: CHORD_INDEX[name] // PAGE_SIZE + 1 for name, _r, _m in GAME_CHORDS}

def _exact_game(root: int, minor: bool, shift: int) -> str | None:
    r = (root + shift) % 12
    for name, croot, cminor in GAME_CHORDS:
        if croot == r and cminor == minor:
            return name
    return None

def _nearest_game(root: int, minor: bool, page: int, prev: str = "") -> str:
    """游戏里没有这个和弦时：在当前页按共同音最多、调式相同、根音最近来近似。"""
    core = {root, (root + (3 if minor else 4)) % 12, (root + 7) % 12}

    def key(item):
        name, croot, cminor = item
        dist = min((root - croot) % 12, (croot - root) % 12)
        return (len(core & GAME_PCS[name]), cminor == minor, -dist, name == prev)

    candidates = [c for c in GAME_CHORDS if GAME_PAGE[c[0]] == page]
    return max(candidates, key=key)[0]

# ============================================================ MIDI 读取

class MidiError(ValueError):
    pass

@dataclass(frozen=True)
class RawNote:
    start: int
    end: int
    note: int
    velocity: int
    channel: int

@dataclass
class MidiFile:
    format: int = 1
    tracks: int = 1
    division: int = 480
    name: str = ""
    notes: list[RawNote] = field(default_factory=list)
    tempos: list[tuple[int, int]] = field(default_factory=list)
    end_tick: int = 0
    unclosed: int = 0

def _read_varlen(data: bytes, pos: int) -> tuple[int, int]:
    value = 0
    for _ in range(4):
        if pos >= len(data):
            raise MidiError("变长整数被截断")
        byte = data[pos]
        pos += 1
        value = (value << 7) | (byte & 0x7F)
        if not byte & 0x80:
            return value, pos
    raise MidiError("变长整数超过 4 字节")

def _parse_track(body: bytes, result: MidiFile) -> None:
    pos = 0
    tick = 0
    running: tuple[int, int] | None = None
    opened: dict[tuple[int, int], list[tuple[int, int]]] = {}
    last_tick = 0

    while pos < len(body):
        delta, pos = _read_varlen(body, pos)
        tick += delta
        if pos >= len(body):
            raise MidiError("事件被截断")
        status = body[pos]

        if status == 0xFF:
            pos += 1
            if pos >= len(body):
                raise MidiError("meta 事件被截断")
            meta = body[pos]
            pos += 1
            length, pos = _read_varlen(body, pos)
            payload = body[pos:pos + length]
            if len(payload) < length:
                raise MidiError("meta 事件长度越界")
            pos += length
            if meta == 0x51 and length >= 3:
                result.tempos.append((tick, int.from_bytes(payload[:3], "big")))
            elif meta == 0x03 and payload and not result.name:
                result.name = payload.decode("utf-8", "replace").strip("\0 ")
            elif meta == 0x2F:
                last_tick = max(last_tick, tick)
                break
            continue

        if status in (0xF0, 0xF7):
            pos += 1
            length, pos = _read_varlen(body, pos)
            pos += length
            if pos > len(body):
                raise MidiError("sysex 长度越界")
            continue

        first = None
        if status & 0x80:
            command = status & 0xF0
            channel = status & 0x0F
            pos += 1
            running = (command, channel)
        else:
            if running is None:
                raise MidiError("running status 没有前置状态字节")
            command, channel = running
            first = status
            pos += 1

        need = 1 if command in (0xC0, 0xD0) else 2
        args = [first] if first is not None else []
        while len(args) < need:
            if pos >= len(body):
                raise MidiError("通道消息被截断")
            args.append(body[pos])
            pos += 1

        if command == 0x90 and args[1] > 0:
            opened.setdefault((channel, args[0]), []).append((tick, args[1]))
        elif command in (0x80, 0x90):
            queue = opened.get((channel, args[0]))
            if queue:
                start, velocity = queue.pop(0)
                result.notes.append(RawNote(
                    start=start, end=max(tick, start + 1), note=args[0],
                    velocity=velocity, channel=channel,
                ))
        last_tick = max(last_tick, tick)

    for (channel, note), queue in opened.items():
        for start, velocity in queue:
            result.unclosed += 1
            result.notes.append(RawNote(
                start=start, end=max(last_tick, start + 1), note=note,
                velocity=velocity, channel=channel,
            ))
    result.end_tick = max(result.end_tick, last_tick)

def read_midi(path: Path) -> MidiFile:
    data = path.read_bytes()
    if len(data) < 14 or data[0:4] != b"MThd":
        raise MidiError("不是 MIDI 文件（缺 MThd 头）")
    header_len = int.from_bytes(data[4:8], "big")
    if header_len < 6 or len(data) < 8 + header_len:
        raise MidiError("MIDI 头长度不对")
    result = MidiFile(
        format=int.from_bytes(data[8:10], "big"),
        tracks=int.from_bytes(data[10:12], "big"),
        division=int.from_bytes(data[12:14], "big"),
    )
    if result.division & 0x8000:
        raise MidiError("不支持 SMPTE 时间基准的 MIDI（只支持 PPQ）")
    if result.division <= 0:
        raise MidiError("MIDI 时间基准为 0")

    pos = 8 + header_len
    while pos + 8 <= len(data):
        if data[pos:pos + 4] != b"MTrk":
            break
        length = int.from_bytes(data[pos + 4:pos + 8], "big")
        body = data[pos + 8:pos + 8 + length]
        if len(body) < length:
            raise MidiError("track 长度越界（文件被截断）")
        pos += 8 + length
        _parse_track(body, result)

    if not result.notes:
        raise MidiError("MIDI 里没有任何音符")
    result.notes.sort(key=lambda note: (note.start, note.note))
    result.tempos.sort()
    return result

# ============================================================ 时间轴

class TempoMap:
    """tick -> 秒，带 tempo 变化。tick 单位与构造时给的 marks 一致。"""

    def __init__(self, marks: list[tuple[int, int]], division: int = DIVISION):
        clean: dict[int, int] = {}
        for tick, tempo_us in marks:
            clean[max(0, int(tick))] = max(1, int(tempo_us))
        clean.setdefault(0, DEFAULT_TEMPO_US)
        self.marks = sorted(clean.items())
        self.division = max(1, int(division))

    def tempo_at(self, tick: int) -> int:
        value = DEFAULT_TEMPO_US
        for mark_tick, tempo_us in self.marks:
            if mark_tick > tick:
                break
            value = tempo_us
        return value

    def seconds_at(self, tick: int) -> float:
        target = max(0, int(tick))
        seconds = 0.0
        prev_tick = 0
        tempo_us = DEFAULT_TEMPO_US
        for mark_tick, mark_tempo in self.marks:
            if mark_tick > target:
                break
            seconds += (mark_tick - prev_tick) * (tempo_us / 1_000_000) / self.division
            prev_tick = mark_tick
            tempo_us = mark_tempo
        seconds += (target - prev_tick) * (tempo_us / 1_000_000) / self.division
        return seconds

    def trimmed(self, trim_tick: int) -> "TempoMap":
        if trim_tick <= 0:
            return self
        base = self.tempo_at(trim_tick)
        marks = [(0, base)]
        marks += [(tick - trim_tick, us) for tick, us in self.marks if tick > trim_tick]
        return TempoMap(marks, self.division)

# ============================================================ 单音排位

@dataclass
class Note:
    start: int
    end: int
    pitch: int
    string: int
    fret: int

    @property
    def page(self) -> int:
        return page_for(self.fret)

def midi_notes(midi: MidiFile, channel: int | None) -> list[Note]:
    """和弦识别用：只做通道/打击乐过滤，不限制音域。"""
    scale = DIVISION / max(1, midi.division)
    result = []
    for note in midi.notes:
        if channel is not None and note.channel != channel:
            continue
        if note.channel == 9:
            continue
        start = max(0, round(note.start * scale))
        end = max(start + 1, round(note.end * scale))
        result.append(Note(start, end, int(note.note), 0, 0))
    result.sort(key=lambda n: (n.start, -n.pitch))
    return result

def page_for(fret: int) -> int:
    if fret <= 0:
        return OPEN_PAGE
    return (fret - 1) // PAGE_SIZE + 1

def fret_direction(fret: int) -> tuple[int, int]:
    if fret <= 0:
        return (0, STICK_MAX)
    angle = math.radians(((fret - 1) % PAGE_SIZE) * 60)
    return round(STICK_MAX * math.sin(angle)), round(STICK_MAX * math.cos(angle))

def positions_for(pitch: int, max_fret: int = MAX_FRET) -> list[tuple[int, int]]:
    result = []
    for string, open_pitch in enumerate(TUNING, start=1):
        fret = int(pitch) - open_pitch
        if 0 <= fret <= max_fret:
            result.append((string, fret))
    return result

def _score(position, prev_page, prev_fret, active_keys, active_strings):
    string, fret = position
    return (
        abs(page_for(fret) - prev_page),
        1 if string in active_strings else 0,
        1 if position in active_keys else 0,
        abs(fret - prev_fret),
        fret,
        -string,
    )

def _assign_group(pitches, prev_page, prev_fret, active_keys, active_strings, max_fret):
    if not pitches:
        return []

    # 整组落在同一页（和弦不跨页）
    for page in sorted(range(1, 6), key=lambda item: (abs(item - prev_page), item)):
        chosen, used = [], set()
        for pitch in pitches:
            candidates = [
                position for position in positions_for(pitch, max_fret)
                if page_for(position[1]) == page and position[0] not in used
            ]
            if not candidates:
                break
            chosen.append(min(candidates, key=lambda p: _score(
                p, prev_page, prev_fret, active_keys, active_strings)))
            used.add(chosen[-1][0])
        if len(chosen) == len(pitches):
            return chosen

    # 单页放不下：逐音选，尽量不重复用弦
    chosen, used = [], set()
    for pitch in pitches:
        candidates = [p for p in positions_for(pitch, max_fret) if p[0] not in used]
        if not candidates:
            candidates = positions_for(pitch, max_fret)
        if not candidates:
            continue
        chosen.append(min(candidates, key=lambda p: _score(
            p, prev_page, prev_fret, active_keys, active_strings)))
        used.add(chosen[-1][0])
    return chosen

@dataclass
class ConvertStats:
    source_notes: int = 0
    output_notes: int = 0
    dropped_simultaneous: int = 0
    skipped_out_of_range: int = 0
    skipped_percussion: int = 0
    dropped_no_position: int = 0
    trimmed_overlaps: int = 0
    page_changes: int = 0
    pages_used: set[int] = field(default_factory=set)

def convert_mono(midi: MidiFile, channel: int | None, keep_all: bool):
    # MIDI 音符 -> (弦, 品)
    stats = ConvertStats()
    scale = DIVISION / max(1, midi.division)
    highest = TUNING[0] + MAX_FRET

    raw: list[tuple[int, int, int]] = []
    source = 0
    for note in midi.notes:
        pitch = int(note.note)
        if channel is not None and note.channel != channel:
            continue
        source += 1
        if note.channel == 9:
            stats.skipped_percussion += 1
            continue
        if pitch < TUNING[-1] or pitch > highest:
            stats.skipped_out_of_range += 1
            continue
        start = max(0, round(note.start * scale))
        end = max(start + 1, round(note.end * scale))
        raw.append((start, end, pitch))

    stats.source_notes = source
    raw.sort(key=lambda item: (item[0], -item[2]))

    planned: list[Note] = []
    active: list[Note] = []
    prev_page = START_PAGE
    prev_fret = 0

    for start, group in groupby(raw, key=lambda item: item[0]):
        by_pitch: dict[int, int] = {}
        for _start, end, pitch in group:
            by_pitch[pitch] = max(by_pitch.get(pitch, 0), end - start)
        ordered = sorted(by_pitch.items(), key=lambda item: -item[0])

        if not keep_all:
            stats.dropped_simultaneous += max(0, len(ordered) - 1)
            ordered = ordered[:1]
        elif len(ordered) > 6:
            stats.dropped_no_position += len(ordered) - 6
            ordered = ordered[:6]

        pitches = [pitch for pitch, _duration in ordered]

        active = [note for note in active if note.end > start]
        active_keys = {(note.string, note.fret) for note in active}
        active_strings = {note.string for note in active}

        positions = _assign_group(
            pitches, prev_page, prev_fret, active_keys, active_strings, MAX_FRET
        )
        ordered = ordered[:len(positions)]

        for (pitch, duration), (string, fret) in zip(ordered, positions):
            note = Note(start=start, end=start + max(1, duration),
                        pitch=pitch, string=string, fret=fret)
            planned.append(note)
            for earlier in active:
                if (earlier.string, earlier.fret) == (string, fret) and earlier.end > start:
                    earlier.end = max(earlier.start + 1, start)
                    stats.trimmed_overlaps += 1
            active.append(note)

        if positions:
            group_page = page_for(positions[0][1])
            stats.page_changes += 0 if group_page == prev_page else 1
            prev_page = group_page
            prev_fret = positions[0][1]

    stats.output_notes = len(planned)
    stats.pages_used = sorted({note.page for note in planned})
    return planned, stats

# ============================================================ 排期

@dataclass(frozen=True)
class Action:
    time: float
    kind: str                       # press / release / stick / rstick
    button: str = ""
    stick: tuple[int, int] | None = None
    tick: int = 0
    string: int = 0
    fret: int = 0
    page: int = 0
    label: str = ""
    unit: bool = False
    chord: str = ""

class PositionPlanner:
    """翻页 + 摇杆提前到位 + 按弦时刻。翻页用 LB/RB。"""

    def __init__(self, actions, conflicts, hints, aim_gap):
        self.actions = actions
        self.conflicts = conflicts
        self.hints = hints
        self.gap_aim = aim_gap
        self.current_page = 1
        self.current_stick = None
        self.last_press_time = 0.0
        self.last_press_at: dict[int, float] = {}

    def plan_page(self, target_page: int, start: float, tick: int, where: str) -> float:
        if target_page == self.current_page:
            return 0.0

        forward = target_page > self.current_page
        name = "RB" if forward else "LB"
        steps = abs(target_page - self.current_page)
        span = steps * (PAGE_TAP_HOLD + PAGE_TAP_GAP)
        switch_at = max(0.0, start - PAGE_LEAD - span)
        busy_until = max(
            (t for page, t in self.last_press_at.items() if page != target_page),
            default=0.0,
        )
        switch_at = max(switch_at, busy_until)
        if switch_at + span > start:
            self.conflicts.append(
                f"{where} 前翻页时间不足（{name} × {steps}）"
            )
        for step in range(steps):
            at = switch_at + step * (PAGE_TAP_HOLD + PAGE_TAP_GAP)
            self.actions.append(Action(
                time=at, kind="page", button=name, tick=tick,
                page=target_page, label=f"翻页 → 第 {target_page} 页（{name}）",
            ))
            self.actions.append(Action(
                time=at + PAGE_TAP_HOLD, kind="release", button=name,
                tick=tick, page=target_page,
            ))
        page_done_at = switch_at + (steps - 1) * (PAGE_TAP_HOLD + PAGE_TAP_GAP)
        self.current_page = target_page
        return page_done_at

    def aim_and_press(self, *, start, tick, stick_target, page_done_at,
                      next_start, where, string=0, fret=0, chord=""):
        press_at = max(start, self.last_press_time)
        if stick_target == self.current_stick:
            return press_at

        after_press = self.last_press_time + 0.001 if self.last_press_time > 0 else 0.0
        aim_at = max(0.0, start - self.gap_aim,
                     min(page_done_at, start), min(after_press, start))
        press_at = max(press_at, aim_at + self.gap_aim)
        if next_start is not None and press_at > next_start:
            press_at = next_start

        actual = max(0.0, press_at - aim_at)
        if actual + 1e-9 < self.gap_aim:
            self.hints.append(
                f"{where}：摇杆到位到按键只有 {actual * 1000:.0f}ms"
                f"（建议 {self.gap_aim * 1000:.0f}ms）"
            )
        self.actions.append(Action(
            time=aim_at, kind="stick", stick=stick_target, tick=tick,
            string=string, fret=fret, page=self.current_page, chord=chord,
        ))
        self.current_stick = stick_target
        return press_at

    def mark_pressed(self, press_at: float, page: int) -> None:
        self.last_press_at[page] = max(self.last_press_at.get(page, 0.0), press_at)
        self.last_press_time = max(self.last_press_time, press_at)

def _mark_sustained(blocks) -> list[bool]:
    sustained = [False] * len(blocks)
    for index, (_start, end, _string, _fret) in enumerate(blocks):
        for later in range(index + 1, len(blocks)):
            if blocks[later][0] >= end:
                break
            sustained[index] = True
            sustained[later] = True
    return sustained

def _next_same_string_start(blocks) -> dict[int, int]:
    result, latest = {}, {}
    for index in range(len(blocks) - 1, -1, -1):
        start_tick, _end, string, _fret = blocks[index]
        if string in latest:
            result[index] = latest[string]
        latest[string] = start_tick
    return result

def build_mono_schedule(notes: list[Note], tempo: TempoMap):
    # 同一刻起音按结束时间排（与上游 note_blocks() 的事件顺序一致）
    blocks = sorted(((n.start, n.end, n.string, n.fret) for n in notes),
                    key=lambda block: (block[0], block[1]))
    sustained = _mark_sustained(blocks)
    next_same = _next_same_string_start(blocks)
    actions: list[Action] = []
    conflicts: list[str] = []
    hints: list[str] = []
    planner = PositionPlanner(actions, conflicts, hints, AIM_GAP)
    last_release: dict[str, int] = {}
    last_press: dict[str, float] = {}

    for index, (start_tick, end_tick, string, fret) in enumerate(blocks):
        start = LEAD_IN + tempo.seconds_at(start_tick)
        end = LEAD_IN + tempo.seconds_at(end_tick)
        target_page = page_for(fret)

        page_done_at = planner.plan_page(
            target_page, start, start_tick, f"{string}弦{fret}品"
        )

        next_start = (
            LEAD_IN + tempo.seconds_at(blocks[index + 1][0])
            if index + 1 < len(blocks) else None
        )
        press_at = planner.aim_and_press(
            start=start, tick=start_tick, stick_target=fret_direction(fret),
            page_done_at=page_done_at, next_start=next_start,
            where=f"{string} 弦 {fret} 品", string=string, fret=fret,
        )

        button = STRING_BUTTONS[string]
        # 同弦上一根音必须先松开，否则这次按下没有边沿，游戏不会重新拨弦
        previous_release = last_release.get(button)
        if previous_release is not None:
            action = actions[previous_release]
            latest = press_at - MIN_PRESS
            if action.time > latest:
                actions[previous_release] = replace(
                    action,
                    time=max(last_press.get(button, 0.0) + MIN_PRESS, latest),
                )

        actions.append(Action(
            time=press_at, kind="press", button=button, tick=start_tick,
            string=string, fret=fret, page=planner.current_page,
            label=f"{string}弦 {fret}品 → {button}", unit=True,
        ))
        last_press[button] = press_at
        planner.mark_pressed(press_at, target_page)

        if sustained[index]:
            release_at = max(end, press_at + MIN_PRESS)
        else:
            release_at = press_at + min(TAP_SECONDS, max(0.02, end - start))

        next_tick = next_same.get(index)
        if next_tick is not None:
            next_start_same = LEAD_IN + tempo.seconds_at(next_tick)
            if next_start_same > start:
                release_at = min(release_at, next_start_same - REPLUCK_GAP)
        release_at = max(release_at, press_at + MIN_PRESS)

        actions.append(Action(
            time=release_at, kind="release", button=button, tick=end_tick,
            string=string, fret=fret, page=planner.current_page,
        ))
        last_release[button] = len(actions) - 1

    priority = {"release": 0, "stick": 1, "page": 1, "press": 2}
    actions.sort(key=lambda a: (round(a.time, 6), priority.get(a.kind, 3)))
    return actions, conflicts, hints

# ============================================================ 和弦识别

@dataclass(frozen=True)
class ChordEvent:
    tick: int
    name: str
    up: bool = False

@dataclass
class ChordDetection:
    events: list[ChordEvent] = field(default_factory=list)
    unmatched: int = 0      # 起音够 3 个音、但识别不出三和弦
    detected: int = 0
    shift: int = 0

def _detect_triad(pcs: set[int], bass: int | None):
    """按三和弦识别 (根音, 是否小三)；七度/九度音当额外音容忍。"""
    best = None
    for root in range(12):
        for minor in (False, True):
            third = 3 if minor else 4
            if (root + third) % 12 not in pcs:
                continue
            core = {root, (root + third) % 12, (root + 7) % 12}
            hit = len(pcs & core)
            if hit < 2:
                continue
            score = hit * 2 - (3 - hit) * 2 - 0.3 * len(pcs - core)
            if bass == root:
                score += 1.5
            if best is None or score > best[0]:
                best = (score, root, minor)
    return best

def detect_chords(notes: list[Note], tempo: TempoMap, min_repeat_gap: float = 0.12,
                  beats_per_chord: int = 2):
    """识别三和弦 -> 游戏和弦，再排成扫弦事件。

    起音按 1/16 拍聚合；和弦按 ``beats_per_chord`` 拍（默认半小节）投票，
    避免在密集伴奏里每一下都换和弦。翻页来不及时就近换成本页的和弦。
    """
    bucket = DIVISION // 16
    window = DIVISION * beats_per_chord
    ordered = sorted(notes, key=lambda n: n.start)
    found = []          # (tick, root, minor, 权重)
    unmatched = 0

    for key, group in groupby(ordered, key=lambda n: n.start // bucket):
        group = list(group)
        pcs = {n.pitch % 12 for n in group}
        if len(pcs) < 3:
            continue
        bass = min(n.pitch for n in group) % 12
        result = _detect_triad(pcs, bass)
        if not result:
            unmatched += 1
            continue
        found.append((key * bucket, result[1], result[2], len(group)))

    if not found:
        return ChordDetection(unmatched=unmatched)

    # 每个窗口投票，并记下第一/最后一个起音的时刻
    window_stats: dict[int, dict] = {}
    for tick, root, minor, weight in found:
        index = tick // window
        data = window_stats.setdefault(index, {"first": tick, "last": tick, "votes": {}})
        data["first"] = min(data["first"], tick)
        data["last"] = max(data["last"], tick)
        data["votes"][(root, minor)] = data["votes"].get((root, minor), 0) + weight

    # 全局移调：让最多和弦正好落在游戏的和弦表里
    shift = max(
        range(12),
        key=lambda s: (
            sum(w for data in window_stats.values()
                for (root, minor), w in data["votes"].items()
                if _exact_game(root, minor, s)),
            -min(s, 12 - s),
        ),
    )

    # 逐窗口选和弦：翻页来不及就退到当前页的近似和弦
    named: dict[int, str] = {}
    current_page = 1
    previous = ""
    last_strum = -1e9
    for index in sorted(window_stats):
        data = window_stats[index]
        root, minor = max(data["votes"].items(), key=lambda item: item[1])[0]
        root = (root + shift) % 12
        first = tempo.seconds_at(data["first"])
        last = tempo.seconds_at(data["last"])

        exact = _exact_game(root, minor, 0)
        target = GAME_PAGE[exact] if exact else 0
        steps = abs(target - current_page) if exact else 0
        needed = steps * (PAGE_TAP_HOLD + PAGE_TAP_GAP) + PAGE_LEAD

        if exact and (target == current_page or first - last_strum >= needed):
            name = exact
            current_page = target
        else:
            name = _nearest_game(root, minor, current_page, previous)
        named[index] = name
        previous = name
        last_strum = last

    result = ChordDetection(detected=len(found), shift=shift, unmatched=unmatched)
    last_name = ""
    last_time = -1e9
    last_up = False
    for tick, root, minor, weight in found:
        name = named.get(tick // window)
        if not name:
            continue
        now = tempo.seconds_at(tick)
        changed = name != last_name
        if not changed and now - last_time < min_repeat_gap:
            continue
        up = False if (changed or now - last_time > 1.2) else not last_up
        result.events.append(ChordEvent(tick=tick, name=name, up=up))
        last_name, last_time, last_up = name, now, up

    return result

def build_chord_schedule(events: list[ChordEvent], tempo: TempoMap):
    actions: list[Action] = []
    conflicts: list[str] = []
    hints: list[str] = []
    planner = PositionPlanner(actions, conflicts, hints, AIM_GAP)

    for event in events:
        slot = CHORD_INDEX.get(event.name)
        if slot is None:
            conflicts.append(f"和弦 {event.name} 不在表里，已跳过")
            continue
        page = slot // PAGE_SIZE + 1
        direction = slot % PAGE_SIZE + 1
        stick_target = fret_direction(direction)
        start = LEAD_IN + tempo.seconds_at(event.tick)

        page_done_at = planner.plan_page(page, start, event.tick, f"和弦 {event.name}")
        at = planner.aim_and_press(
            start=start, tick=event.tick, stick_target=stick_target,
            page_done_at=page_done_at, next_start=None,
            where=f"和弦 {event.name}", fret=direction, chord=event.name,
        )

        push = STRUM_UP if event.up else STRUM_DOWN
        actions.append(Action(
            time=at, kind="rstick", stick=push, tick=event.tick,
            fret=direction, page=page, chord=event.name, unit=True,
            label=f"{event.name} {'上扫' if event.up else '下扫'}",
        ))
        actions.append(Action(
            time=at + CHORD_STRUM_FLICK, kind="rstick", stick=(0, 0),
            tick=event.tick, fret=direction, page=page, chord=event.name,
        ))
        planner.mark_pressed(at, page)

    priority = {"release": 0, "stick": 1, "rstick": 2, "press": 3}
    actions.sort(key=lambda a: (round(a.time, 6), priority.get(a.kind, 4)))
    return actions, conflicts, hints

# ============================================================ rhai 输出

RHAI_BTN = {
    "X": "X", "A": "A", "B": "B", "Y": "Y", "LB": "LB", "RB": "RB",
    "DPAD_UP": "UP", "DPAD_RIGHT": "RIGHT",
    "DPAD_LEFT": "LEFT", "DPAD_DOWN": "DOWN",
}

def emit_rhai(actions: list[Action], *, chord_mode: bool, title: str,
              timeout: int) -> str:
    lines = [
        "// {",
        f'//   "desc": "{title}",',
        '//   "loop": false,',
        f'//   "timeout": {timeout}',
        "// }",
        "",
    ]

    clock = 0.0
    if not chord_mode:
        lines.append("click(RT);   // 和弦模式 -> 品格模式")
        clock += 0.2
    lines.append("click(LT);   // 清音 -> 失真")
    clock += 0.2
    lines.append("")

    if actions:
        first = round(actions[0].time, 3)
        if first - clock > 0.1:
            lines.append(f"wait({first - clock:.3f});")

    times = [round(action.time, 3) for action in actions]
    for index, action in enumerate(actions):
        next_time = times[index + 1] if index + 1 < len(times) else times[index]
        duration = max(0.0, next_time - times[index])

        if action.kind in ("press", "page"):
            lines.append(f"press({RHAI_BTN[action.button]},{duration:.3f});")
        elif action.kind == "release":
            lines.append(f"release({RHAI_BTN[action.button]},{duration:.3f});")
        elif action.kind == "stick":
            x, y = action.stick
            lines.append(f"lstick({x},{y},{duration:.3f});")
        elif action.kind == "rstick":
            x, y = action.stick
            lines.append(f"rstick({x},{y},{duration:.3f});")

    lines.append("")
    lines.append("pad_reset();")
    return "\n".join(lines) + "\n"

def main() -> None:
    ap = argparse.ArgumentParser(
        formatter_class=argparse.RawDescriptionHelpFormatter,
        description="midi->明音凛吉他演奏脚本",
        epilog=(
            "示例：\n"
            "  python 明音凛吉他脚本.py song.mid\n"
            "  python 明音凛吉他脚本.py song.mid -c 10\n"
            "  python 明音凛吉他脚本.py song.mid --chords\n"
        ),
    )
    ap.add_argument("midi", help="输入 .mid")
    ap.add_argument("-o", "--out", help="输出 .rhai（默认与 MIDI 同名同目录）")
    ap.add_argument("-c", "--channel", type=int,
                    help="只取这个 0 基 MIDI 通道")
    ap.add_argument("--keep-all", action="store_true",
                    help="同时起的音全部保留（最多 6 个），默认只留最高音")
    ap.add_argument("--chords", action="store_true",
                    help="和弦扫弦模式（默认单音）")
    args = ap.parse_args()

    midi_path = Path(args.midi)
    if not midi_path.is_file():
        raise SystemExit(f"找不到 {midi_path}")
    suffix = f"-ch{args.channel}" if args.channel is not None else ""
    if args.chords:
        suffix += "-chords"
    out_path = (Path(args.out) if args.out
                else midi_path.with_name(midi_path.stem + suffix + ".rhai"))

    try:
        midi = read_midi(midi_path)
    except MidiError as exc:
        raise SystemExit(f"读 MIDI 失败：{exc}")

    scale = DIVISION / max(1, midi.division)
    tempo = TempoMap(
        [(round(tick * scale), us) for tick, us in midi.tempos], DIVISION
    )

    if args.chords:
        notes = midi_notes(midi, args.channel)
        stats = ConvertStats(source_notes=len(notes))
        if not notes:
            raise SystemExit("没有可用的音符（检查通道）。")
    else:
        notes, stats = convert_mono(midi, args.channel, args.keep_all)
        if not notes:
            raise SystemExit("没有可用的音符（检查通道/音域）。")

    # 砍掉开头静音
    trim = min(note.start for note in notes)
    trim_seconds = midi_tempo_seconds(midi, trim, scale) if trim > 0 else 0.0
    if trim > 0:
        for note in notes:
            note.start -= trim
            note.end -= trim
        tempo = tempo.trimmed(trim)

    if args.chords:
        detection = detect_chords(notes, tempo)
        if not detection.events:
            raise SystemExit("没有识别出和弦；试试别的通道，或改用单音模式。")
        actions, conflicts, hints = build_chord_schedule(detection.events, tempo)
        duration = actions[-1].time if actions else 0.0
        signed = detection.shift - 12 if detection.shift > 6 else detection.shift
        shift = f"{signed:+d}" if signed else "0"
        print(f"源 {stats.source_notes} 音 -> 和弦 {len(detection.events)} 个"
              f"   移调 {shift} 半音   识别 {detection.detected} 个"
              f"   翻页 {sum(1 for a in actions if a.kind == 'page')} 步")
        if detection.unmatched:
            print(f"  · {detection.unmatched} 个起音识别不出和弦")
    else:
        actions, conflicts, hints = build_mono_schedule(notes, tempo)
        duration = actions[-1].time if actions else 0.0
        print(f"源 {stats.source_notes} 音 -> 输出 {stats.output_notes} 音"
              f"   翻页 {sum(1 for a in actions if a.kind == 'page')} 步"
              f"   用到页 {stats.pages_used}")

    if trim > 0:
        print(f"已跳过开头静音 {trim_seconds:.1f}s")

    if stats.skipped_percussion:
        print(f"  ⚠ 跳过打击乐 {stats.skipped_percussion} 音")
    if stats.skipped_out_of_range:
        print(f"  ⚠ {stats.skipped_out_of_range} 音超出 E2–E6，已跳过")
    if stats.dropped_simultaneous:
        print(f"  ⚠ 省略同时起音的 {stats.dropped_simultaneous} 个低音")
    if stats.dropped_no_position:
        print(f"  ⚠ {stats.dropped_no_position} 个同时音超出 6 根弦，已跳过")
    if stats.trimmed_overlaps:
        print(f"  ⚠ {stats.trimmed_overlaps} 个同弦同品的重叠音被截断")
    if conflicts:
        print(f"  ⚠ 冲突 {len(conflicts)} 条")
        for line in conflicts[:5]:
            print(f"    - {line}")
    if hints:
        print(f"  · 提示 {len(hints)} 条")

    code = emit_rhai(
        actions, chord_mode=args.chords,
        title=midi_path.stem + suffix,
        timeout=int(duration + 120),
    )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(code, encoding="utf-8")
    print(f"已写出 {out_path}（{len(code.splitlines())} 行，{duration:.1f}s）")

def midi_tempo_seconds(midi: MidiFile, trim_tick: int, scale: float) -> float:
    """被砍掉的开头静音有多少秒（仅用于显示）。"""
    tempo = TempoMap([(round(t * scale), us) for t, us in midi.tempos], DIVISION)
    return tempo.seconds_at(trim_tick)

if __name__ == "__main__":
    main()
