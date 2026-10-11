# -*- coding: utf-8 -*-
"""위저드 영어 번역표 G6 — 도움말(F1) 튜토리얼 `tutorial_text` 와 도움말 창 문구(2026-09-24).

`tutorial_text` 와 **같은 모양**의 영어 표(`INTRO`·`TUTORIAL`·`CAPTIONS`…)를 두고, 둘을
`tutorial_text.all_strings()` 로 같은 순서로 펴서 (한국어, 영어) 쌍 `PAIRS` 를 만든다 — 한국어를 여기
다시 적지 않으니 문구를 고칠 때 어긋날 일이 없다. 모양(단계 수 · 따라 하기 수 · 그림 번호 수)이 다르면
임포트 때 `AssertionError` 로 멈춘다. `strings.py` 가 다른 묶음과 똑같이 `add_korean` 으로 등록한다.

(계약 문서는 이 파일을 `en_g5` 로 적었지만, `en_g5` 는 같은 날 풍선 도움말 번역이 먼저 썼다.)
"""
from secs.wizard import tutorial_text as _ko

HINT = 'Hover over any field or button to see what it does.'
TAB_INTRO = 'First time?'
# 탭 이름은 짧게 — 520 px 창에서 탭 5개가 스크롤 없이 들어가게(QA 09-24).
TABS = (
    ('steps', 'Steps'),
    ('checks', 'Checks'),
    ('trouble', 'Problems'),
    ('real', 'Real REW'),
)

CAPTIONS = {
    'side_nav': ('Sidebar on the left · tutorial', [
        'Step list — click to jump to that step',
        '[Help] — this window (F1)',
        '[Tutorial] — shows [End tutorial] while in the tutorial',
        '[Save project] · ▾ Save as',
    ]),
    's1_overview': ('Step 1, whole screen', ['REW connection card', 'Microphone · Audio card', '[Next →]']),
    's1_rew': ('REW connection card', [
        'Connect button — [Launch REW in API mode] while REW is closed, [Retry] once connected',
        'Connection status — "API connected" means you are ready',
        '[Read settings]',
    ]),
    's1_mic': ('Microphone · Audio card', ['Input device', 'Input channel', 'Mic calibration file', 'Sweep length']),
    's2_overview': ('Step 2, whole screen — demo room 7.1.4 · 3 subs',
                    ['Preset row', 'Output patch sheet', 'Speaker layout · DSP profile']),
    's2_preset': ('Preset row', ['[7.1.4] preset', 'Subwoofer − / +', '[+ Add speaker]']),
    's2_patch': ('Output patch sheet', ['Measurement output column', 'Polarity column', 'Size column']),
    's2_dsp': ('DSP profile · filter resources', ['DSP device', 'FIR taps · IIR bands']),
    's3_overview': ('Step 3, whole screen', ['Channel levels', 'Master output']),
    's3_channel': ('One channel row', ['▶ play / ■ stop', 'Blue target zone on the level bar', 'Current level (dB)']),
    's3_master': ('Master output card', ['Master slider (REW output level)', '[Play all in sequence]']),
    's4_overview': ('Step 4, whole screen', ['Layout', 'Measurement options']),
    's4_pick': ('Layout and position map',
                ['Position 1 = listening position', 'Arrangement type — Single · Solo · Couple · Family']),
    's4_options': ('Measurement options', ['Timing reference', 'Reference output']),
    's5_before': ('Step 5 — before measuring',
                  ['Current position (Position 1 / 1)', '[Measure this position]', 'Position list']),
    's5_running': ('Step 5 — measuring', ['Progress — elapsed · estimate · remaining']),
    's5_after1': ('Step 5 — after position 1', [
        'Curves of the channels just measured',
        '"Done" in the position list',
        '[All positions done] — the one-point sample ends here',
    ]),
    's5_done': ('Step 5 — all positions done', ['Every position "Done"', '[Next →] enabled']),
    's5_mdat': ('Load mdat instead of measuring', [
        '[Load mdat] — bring in measurements made in REW, no REW Pro needed',
        'Start delay — the wait before each sweep when measuring for real',
    ]),
    's9_overview': ('Room correction, whole screen', [
        'Correction mode · [Calculate]',
        'Tabs — SPL · GD · XO · Sub alignment',
        'Graph — before · after · target',
        'Settings card that follows the tab',
    ]),
    's9_after': ('After calculating', ['"Calculated" and the total latency', 'SPL graph — corrected curve and target (dashed)']),
    's9_spl': ('RC range card — SPL · GD tabs', [
        'RC low · high (auto)',
        'Room volume · dimensions — set the auto upper limit',
        'Boost limit — headroom',
    ]),
    's9_xo': ('XO tab', [
        'Channel — main · sub · sum of the chosen channel',
        'XO frequency',
        'XO PEQ on · low · high (empty = auto)',
        '[Recommend XO]',
    ]),
    's9_subs': ('Sub alignment tab', [
        'Infra sub · infra XO · infra max boost',
        'Mode per sub — auto · fixed · Group',
        '[Subs only]',
        'Each sub: dashed before alignment, solid after · the black line is the sub sum',
    ]),
    's8_overview': ('Export, whole screen', ['Summary of the result to export', 'Export folder', 'Export buttons']),
    's8_export': ('After exporting', [
        '[Save REW mdat]',
        '[Export CamillaDSP]',
        'Number of files saved and the folder path',
        '[Open folder]',
    ]),
    's8_files': ('Inside the exported folder', ['REW mdat — before · after · MIMO', 'CamillaDSP config — yml in the configs folder',
                                                'FIR — wav in the coeffs folder']),
}

INTRO = {
    'title': 'First time? — one lap in tutorial',
    'body': [
        'Go from the start to the export once, with no microphone and no REW. Real measuring is much easier once you know the flow.',
        'Press [Tutorial] in the sidebar and this window switches to the tutorial. It is the same as the sample-data .cmd '
        'launcher in the release folder, and your current project stays as it is.',
        'During the tutorial the sidebar, step bar and accent buttons turn pink and the window title starts with '
        '[Tutorial]; nothing makes a sound and nothing is saved or exported. The curves come from a real measurement of '
        'a 7.1.4 · 3 subs demo room at the listening position.',
        'First note where the step list, [Help], [Tutorial] and [Save project] sit in the sidebar.',
        'Without REW Pro, start from [Load mdat] on the 5 Measure screen. Device connection and level '
        'playback in the earlier steps, and automatic measurement in step 5, are REW Pro (API) features.',
    ],
    'img': 'side_nav',
    'route': [
        '1 Devices — already connected. Press [Next →].',
        '2 Setup — the 7.1.4 · 3 subs demo room is already filled in. Just press [Next →].',
        '3 Level check — press [Play all in sequence], then [Next →].',
        '4 Positions — keep Single (listening position), then [Next →].',
        '5 Measure — press [Measure this position]; the fake REW fills it at once. Then [Next →].',
        '6 Room correction — press [Calculate] and flip through the SPL, XO and Sub alignment tabs. Then [Next →].',
        '7 Export — the buttons are locked in the tutorial. In a real project use [Save REW mdat] · [Export CamillaDSP].',
        'Done — press [End tutorial] in the sidebar to get your project back.',
    ],
    'outro': [
        'Press F1 on any step to open its walkthrough with pictures. You can keep working while it stays open.',
        'For real measurements use the REW launcher (the .cmd file whose Korean name means "SECS wizard (with REW)") '
        'and start REW first as roomeqwizard.exe -api.',
    ],
}

TUTORIAL = {
    1: {
        'goal': 'Make sure SECS is talking to REW and see which input the measurement mic comes in on.',
        'steps': [
            {'do': 'Look at the two cards.',
             'see': 'REW connection is on the left, Microphone · Audio on the right. [Next →] is at the bottom right.',
             'img': 's1_overview'},
            {'do': 'Press the first button on the REW connection card.',
             'see': 'While REW is closed it reads [Launch REW in API mode] and starts REW (10–20 s). '
                    'Move on when the status turns green: "API connected · 127.0.0.1:4735".',
             'img': 's1_rew'},
            {'do': 'Check the Pro license line.',
             'see': 'If it says "Confirmed · auto measurement", SECS can run the sweeps itself.',
             'img': 's1_rew'},
            {'do': 'Set Input device and Input channel to your measurement mic.',
             'see': 'The lists come from REW. Move on once you see your mic.',
             'img': 's1_mic'},
            {'do': 'Check Mic calibration file and Sweep length.',
             'see': 'You should see the mic calibration file name. Leave the sweep length at the default 256k · 5.5 s.',
             'img': 's1_mic'},
            {'do': 'Press [Next →].',
             'see': 'You move on to step 2, Setup. You can always come back from the step list in the sidebar.',
             'img': 'side_nav'},
        ],
        'checks': [
            'REW must be running with the -api option. A normally started REW will not connect.',
            'Make sure the input channel is the one the mic is plugged into. Otherwise the step 3 bars will not move.',
            'The mic calibration file is loaded in REW. SECS only reads it and shows it.',
        ],
        'trouble': [
            ('The status stays "Not connected".',
             'If REW is running without -api, close it and press [Launch REW in API mode] again. '
             'Once the REW window is up, press [Retry].'),
            ('The input device list is empty.',
             'Once REW is connected, press [Retry] to read the lists again.'),
            ('[Next →] is disabled and says there is no Pro license.',
             'SECS cannot start measurements. Use [Skip device check], measure in REW yourself, and load the mdat '
             'in step 5 with [Skip measurement (load mdat)].'),
            ('[Read settings] shows "unknown".',
             'REW did not report that value. Check it in REW itself.'),
        ],
        'real': [
            'With sample data a fake REW is always connected. For real, start REW first as roomeqwizard.exe -api.',
            'SECS runs the sweeps itself only with a REW Pro license. Without one, measure in REW and let SECS read the mdat.',
            'Input device, channel and mic calibration file are read straight from REW. If REW uses a different '
            'mic calibration file per input, these values alone cannot tell you which.',
        ],
    },
    2: {
        'goal': 'Write down which speaker is wired to which DSP output. This table sets the measurement order '
                'and the export channels.',
        'steps': [
            {'do': 'Read the screen from the top.',
             'see': 'Preset row, output patch sheet, then speaker layout and DSP profile. Scroll to see the lower part.',
             'img': 's2_overview'},
            {'do': 'Press your room layout in the preset row. The sample data is [7.1.4] with 3 subs.',
             'see': 'The output patch sheet fills with speaker rows: 12 for 7.1.4, and 14 for the sample with its two extra subs.',
             'img': 's2_preset'},
            {'do': 'Set the number of subs with Subwoofer − / +.',
             'see': 'Each sub adds or removes one row. The sample data has 3.',
             'img': 's2_preset'},
            {'do': 'If a speaker is missing, press [+ Add speaker].',
             'see': 'A new row appears at the bottom. Remove unwanted rows with the × at the end of the row.',
             'img': 's2_preset'},
            {'do': 'Match each row\'s measurement output number to the real wiring.',
             'see': 'You are fine when the badge under the table is a green "No duplicate outputs".',
             'img': 's2_patch'},
            {'do': 'Check the Polarity and Size columns.',
             'see': 'Change polarity only for speakers wired in reverse. Size can stay at the preset value.',
             'img': 's2_patch'},
            {'do': 'Scroll down and pick your DSP device in the DSP profile.',
             'see': 'FIR taps and IIR bands change to match that device. Filters in Room correction and Export are sized to fit.',
             'img': 's2_dsp'},
            {'do': 'Press [Next →].',
             'see': 'You move on to step 3, Level check.',
             'img': None},
        ],
        'checks': [
            'The measurement output number is the output of the audio device REW sweeps through. '
            'Patch the DSP inputs to the same numbers.',
            'Check that the "No duplicate outputs" badge is green.',
            'For bi-amped speakers, turn on "Mains start as bi-amp". Leave it off for the sample data.',
            'The measured sample curves exist only for 7.1.4 · 3 subs · bi-amp off. Other layouts are filled with synthetic curves.',
        ],
        'trouble': [
            ('The measurement audio device list is empty.',
             'It fills only once REW is connected in API mode in step 1. Go back to step 1 and press [Retry].'),
            ('[Next →] is disabled.',
             'The text in the middle at the bottom says why. Usually an output number is duplicated or a row is empty.'),
            ('Room correction filter lengths do not suit your hardware.',
             'Check that the DSP profile matches your device. Filter length comes from the FIR tap count here.'),
        ],
        'real': [
            'For real, the audio interface and DSP must be wired exactly as the measurement output numbers say. '
            'A wrong number plays a different speaker.',
            'Save the table with [Patch sheet CSV] and use it to patch the DSP inputs.',
            'The DSP profile must match your device\'s FIR tap and IIR band limits so the Export values can go in as they are.',
        ],
    },
    3: {
        'goal': 'Put the mic at the listening position and play each channel to set the level.',
        'steps': [
            {'do': 'Place the mic at ear height at the listening position.',
             'see': 'The channel list is on the left, the master output on the right.',
             'img': 's3_overview'},
            {'do': 'Start with the master slider low.',
             'see': 'Start quiet and raise it bit by bit.',
             'img': 's3_master'},
            {'do': 'Press ▶ on the first channel.',
             'see': 'The button turns into ■ and plays for 2.5 s. You are fine when the bar lands in the blue '
                    'target zone (-30 to -15 dB).',
             'img': 's3_channel'},
            {'do': 'If the bar falls short or overshoots, move the master slider and press ▶ again.',
             'see': 'The value at the end of the row shows the current level (dB).',
             'img': 's3_channel'},
            {'do': 'Press [Play all in sequence].',
             'see': 'Channels play one after another and the values fill in. You are done when the '
                    '"Level not measured yet" note at the bottom disappears.',
             'img': 's3_master'},
            {'do': 'Press [Next →].',
             'see': 'You move on to step 4, Positions.',
             'img': None},
        ],
        'checks': [
            'Check that every channel is inside the blue zone. If one channel is far lower, check its wiring and amp first.',
            'REW output above -6 dBFS has to be unlocked in REW. Start low to protect the speakers.',
            'Mic gain is set in REW or on the audio interface, not in SECS.',
            '[Skip levels (use defaults)] treats every channel as in range. Use it only to see the flow.',
        ],
        'trouble': [
            ('The bar does not move at all.',
             'Check that the input device and channel in step 1 match the mic.'),
            ('One channel makes no sound.',
             'Compare its measurement output number in step 2 with the real wiring.'),
            ('The bar goes past the blue zone.',
             'Lower the master slider. Too loud and the sweeps may distort.'),
        ],
        'real': [
            'With sample data nothing plays; you only see fake levels.',
            'For real, pink noise plays, a 50 Hz sine on the subs, and REW reads the mic input. '
            'A quiet room keeps the reading steady.',
            'The master slider sets the REW output level (dBFS). The step 5 sweeps use this level too.',
        ],
    },
    4: {
        'goal': 'Choose how many measurement positions to use and in what shape. Position 1 is always the listening position.',
        'steps': [
            {'do': 'Look at Layout on the left and Measurement options on the right.',
             'see': 'When you pick an arrangement, the map below shows the position numbers.',
             'img': 's4_overview'},
            {'do': 'Press the arrangement that fits your seating.',
             'see': 'Keep Single, the listening position only, for the sample. For real measurements pick Solo with 9 '
                    'positions for a single chair, or Couple with 13 or Family with 17 for a sofa.',
             'img': 's4_pick'},
            {'do': 'Find position 1 on the map.',
             'see': 'Position 1 is the listening position (sweet spot) and the reference for level and delay '
                    'alignment. Half-filled circles are spots where the mic goes 20 cm higher or lower.',
             'img': 's4_pick'},
            {'do': 'Check Timing reference and Reference output.',
             'see': 'Keep the default "Acoustic (ref. speaker)". The reference output must not be a sub.',
             'img': 's4_options'},
            {'do': 'Check the time estimate.',
             'see': 'It shows the time per position and the total. Time spent moving the mic is not included.',
             'img': 's4_options'},
            {'do': 'Press [Next →].',
             'see': 'You move on to step 5, Measure.',
             'img': None},
        ],
        'checks': [
            'Spread the points at least 30 cm apart, over at least 1 m across.',
            'Vary the height too, ear height ± 20 cm.',
            'Real measurements should use at least 5 positions. Single (one point) is a quick check, and the sample uses it.',
            'With the timing reference set to "None", distance, delay and sub alignment cannot be calculated.',
        ],
        'trouble': [
            ('The time estimate is too long.',
             'Pick an arrangement with fewer positions, or shorten the sweep in step 1. Shorter sweeps are more sensitive to noise.'),
            ('The speaker you want is missing from the reference output list.',
             'The list follows the step 2 setup. A sub cannot be the reference.'),
            ('Loopback is selected but the timing looks wrong.',
             'A spare output must be cabled to a spare input, and REW\'s reference input must be set in REW.'),
        ],
        'real': [
            'For real you move the mic for every position. Turning on the start delay gives you time to get to '
            'the spot after pressing [Measure this position].',
            'With the acoustic reference, the reference speaker plays a short timing tone before each sweep. '
            'Hearing an extra sound is normal.',
            'Mixing in a few spots 20–30 cm outside the sofa prevents over-correction.',
        ],
    },
    5: {
        'goal': 'Place the mic at the position shown and sweep every channel at each position.',
        'steps': [
            {'do': 'Look at "Position 1 / 1" at the top and the position list on the right.',
             'see': 'The spot to measure is highlighted in the list. The sample has only position 1 (the listening position).',
             'img': 's5_before'},
            {'do': 'Put the mic there and press [Measure this position].',
             'see': 'Each channel is swept in sequence.',
             'img': 's5_before'},
            {'do': 'Wait for the measurement to finish.',
             'see': 'The progress box at the bottom right shows elapsed, estimated and remaining time. '
                    'Press [Cancel] to stop.',
             'img': 's5_running'},
            {'do': 'Look at the graph and the position list.',
             'see': 'Channel curves appear on the graph and 01 in the list turns "Done".',
             'img': 's5_after1'},
            {'do': 'If there are more positions, move the mic and press [Measure this position] again.',
             'see': 'The sample has one point, so it ends here and the button changes to [All positions done].',
             'img': 's5_after1'},
            {'do': 'Check that every position in the list is "Done".',
             'see': 'The button changes to [All positions done] and [Next →] becomes active.',
             'img': 's5_done'},
            {'do': 'Without REW Pro, press [Load mdat] instead of measuring.',
             'see': 'Measure each channel yourself in REW, name the measurements L · R · C · SW1 …, save as .mdat and pick the '
                    'file. A window opens with the channels filled in by name; if it looks right, press [Load]. '
                    'More in the "Load mdat" tab.',
             'img': 's5_mdat'},
            {'do': 'Press [Next →].',
             'see': 'You move on to Room correction.',
             'img': None},
        ],
        'checks': [
            'Once measuring starts, the project is saved automatically in Documents\\SECS. '
            'The file name is shown under Project in the sidebar.',
            'To redo a bad position, click its row in the list and measure again; only that record is replaced.',
            'If a warning says the signal-to-noise ratio is below 40 dB, raise the level or lengthen the sweep.',
            'You cannot change position while measuring.',
            'You can save and load one mdat per position. For position 2, end the measurement names with _P2. Example: L_P2.',
        ],
        'trouble': [
            ('The curves are ragged and look like noise.',
             'Look for the signal-to-noise warning. Raise the level in step 3 or lengthen the sweep in step 1.'),
            ('One channel\'s curve is far too low.',
             'Check its output number and wiring in step 2. After fixing, measure that position again.'),
            ('The last position went wrong.',
             '[Redo last] deletes that position and measures it again.'),
            ('The measurement never finishes.',
             'Stop it with [Cancel] and check the REW connection with [Retry] in step 1.'),
        ],
        'real': [
            'Real sweeps take channels × sweep length. For 7.1.4 at 256k that is about a minute and a half per position.',
            'Keep the room quiet and leave the mic alone during the sweeps.',
            'Without REW Pro, measure in REW yourself and bring it in with [Load mdat] — the results are the same as measuring through SECS.',
            'The sample is one real point measured in the demo room (the listening position). For real you measure 5+ positions, and the next position is selected each time one finishes.',
        ],
    },
    8: {
        'goal': 'Export the result as a REW mdat that compares before and after, and as a CamillaDSP configuration.',
        'steps': [
            {'do': 'Check the summary at the top to see what will be exported.',
             'see': 'The correction mode, channels, RC range and whether MIMO is included are shown.',
             'img': 's8_overview'},
            {'do': 'Press [Save REW mdat].',
             'see': 'One mdat file named after the correction mode is saved; open it in REW with File > Open. Per channel the '
                    'order is <nn>_<ch>_1 before · _2 after · _3 + MIMO. _1 is the listening-position measurement as is, _2 the '
                    'calculated channel correction and sub sum, and _3 appears only when MIMO was calculated. The subs, as '
                    'measured, come last with their DSP values.',
             'img': 's8_export'},
            {'do': 'Press [Export CamillaDSP].',
             'see': 'The camilladsp folder gets configs with the config secs_<mode>.yml and coeffs with the 48 kHz FIR wav '
                    'files. It has the same layout as a CamillaDSP folder, so you can copy them as they are. The yml holds input '
                    'PEQ · FIR, bass-management XO, sub bus gain · delay · polarity · PEQ and the LFE at +10 dB · 120 Hz to every '
                    'sub, plus the MIMO support paths when MIMO was calculated.',
             'img': 's8_export'},
            {'do': 'Press [Open folder] to see where they went.',
             'see': 'Under SECS_export next to the project, one folder per export named kind · RC_BA(_MIMO) · mode · time.',
             'img': 's8_files'},
            {'do': 'How to load it into CamillaDSP is on the "CamillaDSP" tab.',
             'see': 'The devices are test files (in.raw · out.raw); change them to the loopback and the interface in the GUI. '
                    'For routing, change only the numbers in route_in · route_out on the Mixers tab.',
             'img': None},
        ],
        'checks': [
            'Without a result the buttons are off — press [Calculate] in Room correction first.',
            'With MIMO calculated, the folder name gets _MIMO and the mdat gets "+ MIMO" measurements.',
            'The mdat level uses the REW SPL offset recorded when measuring.',
        ],
        'trouble': [
            ('The buttons cannot be pressed.',
             'Make sure [Calculate] finished in Room correction. The tutorial does not export.'),
            ('The mdat level looks wrong in REW.',
             'Check the measurements came from REW so the SPL offset was recorded. A loaded mdat keeps its own offset.'),
        ],
        'real': [
            'Load the exported config into CamillaDSP as is. Switch the amps off before changing it.',
            'After applying, measure again at the listening position and compare with the corrected curve in the mdat.',
        ],
    },
    9: {
        'goal': 'Pick a correction mode, press [Calculate], then flip through the SPL, GD, XO and Sub alignment tabs and change '
                'only what you need. A change only shows "Settings changed"; press [Calculate] to recalculate.',
        'steps': [
            {'do': 'Pick a correction mode and press [Calculate].',
             'see': 'The mode caps the delay the correction filters may add. Minimum latency corrects magnitude only, while Low '
                    'latency and Standard spend more delay to correct the bass phase too. The first [Calculate] runs the XO '
                    'recommendation first, and the status line shows progress k/n and the time left.',
             'img': 's9_overview'},
            {'do': 'On the SPL tab look at the before and after curves and the dashed target.',
             'see': 'The grey-shaded outside is the band RC leaves alone. Inside it, a good correction has the after curve '
                    'following the target evenly; narrow, deep dips are deliberately not filled all the way. Drag the points to '
                    'change the target, or right-click an empty spot for the Flat, Harman or Cinema preset.',
             'img': 's9_after'},
            {'do': 'Look at the GD tab and the result lines under the graph.',
             'see': 'GD is how late each frequency arrives. The lower the bass peaks and the flatter the curve, the better. '
                    '"Within ±1 dB" is the share of 20–200 Hz within ±1 dB of the target, and "GD spread" is how widely the '
                    'group delay spreads over 20–100 Hz.',
             'img': None},
            {'do': 'In the RC range card set the band to correct and the boost limit.',
             'see': 'Auto high comes from the room volume or dimensions. The boost limit caps every raise by RC, XO PEQ and the sub bus PEQ.',
             'img': 's9_spl'},
            {'do': 'On the XO tab pick a channel and check its XO and XO PEQ.',
             'see': 'The main, sub and sum of that channel are drawn and the card follows its group. A good XO has the sum '
                    'following the target with no dip or bump around the crossover. Drag the vertical line to move the XO; '
                    'XO PEQ high auto = XO × 4.',
             'img': 's9_xo'},
            {'do': 'With two or more subs, look at the graph and value table on the Sub alignment tab.',
             'see': 'One color per sub: dashed is before alignment, solid is after, and the thick black line is the sub sum. The '
                    'flatter the sub sum and the smaller "Sub sum flatness" and "Seat-to-seat spread", the better. The value '
                    'table holds the gain, delay, polarity and all-pass to enter in the DSP for each sub.',
             'img': 's9_subs'},
            {'do': 'On the Sub alignment setup card set the infra sub and each sub\'s mode.',
             'see': 'In the sample SW3 is an infra sub that plays only the deepest bass; picking it puts an LP on SW3 and the '
                    'same HP on the other subs. Auto lets the engine find the values, Fixed uses the values in the fields as '
                    'they are, and Group keeps the relative values of grouped subs. After a change, [Subs only] recalculates '
                    'just this tab in a few seconds.',
             'img': 's9_subs'},
            {'do': 'With 3 or more positions you can tick Use MIMO and press [Calculate MIMO].',
             'see': 'It takes minutes; when done, the SPL tab shows the MIMO curve. When you have seen everything, press '
                    '[Next →] to go to Export.',
             'img': None},
        ],
        'checks': [
            'Changing a setting such as mode, range, target, crossover or sub values shows "Settings changed — press '
            '[Calculate]". Nothing recalculates by itself.',
            '[Take result values] does not calculate; it only copies the last result\'s sub gain, delay, polarity and all-pass into the fields. '
            'A [Subs only] result shows only on the Sub alignment tab; press [Calculate] again to use it for the export.',
            'Grouped subs keep the gain, delay, polarity and all-pass you entered as their relative values; the engine sets '
            'only the Group\'s common gain, delay and polarity.',
            '[Recommend XO] runs the full correction for each candidate crossover, puts the best value per group in and '
            'recalculates. It takes minutes.',
            '[Calculate MIMO] is a separate, heavy calculation that reduces low-frequency decay. A new [Calculate] clears the '
            'MIMO result on top of it, so press it again if you need it.',
        ],
        'trouble': [
            ('"No measurements — measure in step 5" appears.',
             'Measure in step 5 or load an mdat file.'),
            ('[Calculate MIMO] cannot be pressed.',
             'It needs 3 or more measurement positions, MIMO ticked, and a correction result from [Calculate] first.'),
            ('The calculation takes too long.',
             'Stop it with [Cancel calculation]. Standard mode, XO recommendation and MIMO take long.'),
        ],
        'real': [
            'The more positions and channels, the longer the calculation. While it runs, the status line shows the expected time.',
            'Sub alignment looks for each sub’s polarity and delay within ±12 ms of its arrival, and besides flatness it '
            'checks how well the subs add up, the summing efficiency — less cancellation means more low-end max SPL from '
            'the same amps.',
            'The graph and numbers are calculated. Enter them in the DSP, then measure again to check.',
        ],
    },
}


# ──── 모양 검사 — 한국어 표와 단계 수·항목 수·그림 키가 같아야 짝이 맞는다 ────
def _shape(intro, tutorial, captions):
    return (
        (len(intro['body']), len(intro['route']), len(intro['outro']), intro['img']),
        tuple((key, len(captions[key][1])) for key in _ko.CAPTIONS),
        tuple((n, tuple(a['img'] for a in tutorial[n]['steps']), len(tutorial[n]['checks']),
               len(tutorial[n]['trouble']), len(tutorial[n]['real'])) for n in sorted(_ko.TUTORIAL)),
    )


assert set(CAPTIONS) == set(_ko.CAPTIONS) and set(TUTORIAL) == set(_ko.TUTORIAL), 'en_g6: 키가 tutorial_text 와 다르다'
assert _shape(INTRO, TUTORIAL, CAPTIONS) == _shape(_ko.INTRO, _ko.TUTORIAL, _ko.CAPTIONS), \
    'en_g6: 모양이 tutorial_text 와 다르다'

#: 도움말 창(help_dialog)·사이드바 문구.
_WINDOW_PAIRS: tuple[tuple[str, str], ...] = (
    ('이 단계 따라 하기 (F1)', 'Follow this step (F1)'),
    ('그림을 누르면 크게 봅니다.', 'Click a picture to enlarge it.'),
    ('그림 준비 중', 'Picture not available'),
    ('그림 속 번호', 'Numbers in the picture'),
    ('처음 쓰신다면 1단계 도움말의 "처음이라면" 탭부터 보세요.',
     'New to SECS? Start with the "First time?" tab in the step 1 help.'),
    ('샘플 데이터로 1→7 한 번에', 'Steps 1→7 in one go with sample data'),
    ('증상', 'Symptom'),
    ('점검', 'What to check'),
    ('큰 그림', 'Enlarged picture'),
    ('닫으려면 누르거나 Esc', 'Click or press Esc to close'),
)

#: 단계별 덧붙임 탭(10-09) · mdat 불러오기 · CamillaDSP(10-10) — `tutorial_text.EXTRA_TABS` · `NAMES` · `CAMILLA` … 와 같은 모양.
EXTRA_TABS = {5: (('names', 'Load mdat'),), 8: (('camilla', 'CamillaDSP'),)}
NAMES = {
    'start': [
        'No REW Pro? Start here.',
        'Device link, level playback and automatic measurement are REW Pro features. Without it, measure in REW '
        'yourself, save as .mdat, then press [Load mdat] on the measure screen.',
    ],
    'sections': [
        {'head': 'Name your measurements in REW like this.', 'before': [], 'img': 'mdat_names', 'rows': [], 'after': []},
        {'head': 'Pick the file and this window opens.', 'before': [], 'img': 'mdat_dialog', 'rows': [], 'after': []},
        {'head': 'You can save one file per position.',
         'before': ['Press [Load mdat] twice and the files go to positions 1 and 2 in sequence.'],
         'img': 'mdat_files', 'rows': [], 'after': []},
        {'head': 'Write channel names as in this table.', 'before': [], 'img': 'mdat_channels', 'rows': [],
         'after': ['SUB · SW · LFE without a number means SW1.', 'Case, spaces and symbols are ignored.']},
        {'head': 'These names are not recognised automatically.', 'before': [], 'img': None,
         'rows': [('L EQ', 'An extra EQ after the L.'),
                  ('DBA L', 'An extra DBA before the L.'),
                  ('INFRA', 'Not a channel name.')],
         'after': ['Just pick the channel in the window.']},
        {'head': 'Good to know', 'before': [], 'img': None, 'rows': [],
         'after': ['When measuring in REW, set the timing reference to Acoustic. That keeps the time offset between channels.',
                   'The number of positions is set in step 4. Higher position numbers are skipped.']},
    ],
}
NAMES_CAPTIONS = {
    'mdat_names': ('Measurement name = channel name + position number', [
        'Position 1: the channel name only.',
        'Position 2: add _P2 after the name. Position 3: _P3.',
    ]),
    'mdat_dialog': ('The window that opens after you pick the file', [
        'Measurement names, as set in REW',
        'SECS fills in the channel from each name',
        'Names it cannot read show [Skipped]. Pick the channel yourself to use them',
        'If it looks right, press [Load]',
    ]),
    'mdat_files': ('With two files', [
        'First [Load mdat] — Position1.mdat goes into position 1',
        'Second [Load mdat] — Position2.mdat goes into position 2',
    ]),
    'mdat_channels': ('', []),
}
assert _ko.names_shape(NAMES, NAMES_CAPTIONS) == _ko.names_shape() and set(EXTRA_TABS) == set(_ko.EXTRA_TABS), \
    'en_g6: mdat 불러오기 모양이 tutorial_text 와 다르다'

CAMILLA = {
    'start': [
        'Play the SECS correction through CamillaDSP — from a bare PC to sound.',
        'Windows and Mac follow the same steps. Written for CamillaDSP 4.1; screen names may differ a little in other versions.',
    ],
    'sections': [
        {'head': 'Four things you need', 'before': [], 'img': 'cam_kit', 'rows': [], 'after': []},
        {'head': 'Signal path', 'before': [], 'img': 'cam_path', 'rows': [],
         'after': ['Send the player to the loopback. Windows: make CABLE Input the default playback device. Mac: set the sound '
                   'output to BlackHole.',
                   'On Windows, VB-CABLE is a sure thing for stereo. For 12 channels such as 7.1.4, a Mac with BlackHole 16ch is easier.']},
        {'head': 'Make the camilladsp folder first', 'before': [], 'img': 'cam_folder', 'rows': [],
         'after': ['Without configs and coeffs the GUI closes right after it starts.',
                   'If your user name has non-English letters, make it in an English place such as C:\\camilladsp and fix the paths '
                   'in the GUI settings file: gui\\camillagui_backend\\_internal\\config\\camillagui.yml']},
        {'head': 'Put in the files SECS exported',
         'before': ['The exported camilladsp folder has the same layout as the CamillaDSP folder. Copy the files in both folders as they are.'],
         'img': 'cam_export', 'rows': [], 'after': []},
        {'head': 'Start order — two windows + the browser', 'before': [], 'img': 'cam_start', 'rows': [],
         'after': ['Run the commands in the camilladsp folder. On Windows, type cmd in that folder\'s address bar.',
                   '-g-20 means start at −20 dB volume. Keep the amps low the first time too.',
                   'On a Mac, allow "Terminal would like to access the microphone". If you refuse, you get silence with no error.']},
        {'head': 'Or upload with the GUI — Files tab', 'before': ['If you copied the files, just press ★.'],
         'img': 'cam_files', 'rows': [],
         'after': ['Without the wav files a config still shows Valid but fails when applied. So upload the wav files first.',
                   'A file with the same name is overwritten without asking.']},
        {'head': 'Devices — Devices tab', 'before': [], 'img': 'cam_devices', 'rows': [],
         'after': ['Pick device names with the magnifier. The list appears only while the CamillaDSP window is running.',
                   'On a Mac: type CoreAudio, capture BlackHole 16ch, leave the resampler empty.',
                   'Leave format empty. F32_LE gives an error.',
                   'On Windows, set Sound control panel › device Properties › Advanced › Default Format to 48000 Hz on both devices.',
                   'Start chunksize at 1024. If it drops out use 2048; for less latency 512.']},
        {'head': 'Channel table — top of the config file', 'before': [], 'img': 'cam_order', 'rows': [],
         'after': ['Numbers start at 0. Interface output 1 = 0.',
                   'Saving in the GUI removes this comment. The same order is in the Title tab description.']},
        {'head': 'Player order is different — Mixers tab route_in',
         'before': ['Most players send LFE 4th. SECS inputs have LFE last.'],
         'img': 'cam_route_in', 'rows': [],
         'after': ['Click a cell and a small window opens. Delete the link, then press + in the new cell.',
                   'The Windows 7.1 order is FL FR FC LFE BL BR SL SR — back comes before side.']},
        {'head': 'Wiring is different — route_out',
         'before': ['Example: sub on interface output 4, surrounds on 5 · 6.'],
         'img': 'cam_route_out', 'rows': [],
         'after': ['If you change out, set the playback channels on Devices to the same number.', 'Outputs left empty are silent.']},
        {'head': 'Apply and check', 'before': [], 'img': 'cam_check', 'rows': [],
         'after': ['Play something and sound one channel at a time to check the right speaker plays. Use a channel test file or '
                   'the REW signal generator.',
                   'If Clipped samples goes up, turn down the player or the GUI Volume.',
                   'When it works, Save to file. From then on, starting the CamillaDSP window plays the last config right away.']},
        {'head': 'Latency — video sync', 'before': [], 'img': 'cam_latency', 'rows': [],
         'after': ['Each correction mode delays the sound. The value is at the top of the config file and in the Title tab.',
                   'For video, move the player\'s audio sync earlier by that much. Ultra latency 200 ms → −200 ms.']},
        {'head': 'Common problems', 'before': [], 'img': None, 'rows': [],
         'after': ['GUI closes at once → make configs and coeffs folders inside camilladsp.',
                   'State is Offline → the CamillaDSP window is closed or was started without -p1234.',
                   'State is STALLED → the player is sending to a device other than the loopback.',
                   'RUNNING but silent → Mac: microphone permission. Windows: default playback device and 48000 Hz.',
                   'wrong number of channels → make the device channels equal to route_in in and route_out out.',
                   'unknown variant F32_LE → leave the device format empty.',
                   'Drops every few minutes → turn on Rate adjust, on Windows with the AsyncSinc resampler. Still dropping: raise chunksize.',
                   'Unable to find coefficient file → check the wav files are in the coeffs folder.']},
    ],
}
CAMILLA_CAPTIONS = {
    'cam_kit': ('What you need', [
        'CamillaDSP — release 4.1.3 at github.com/HEnquist/camilladsp. Just unzip',
        'CamillaGUI — 4.1.0 bundle at github.com/HEnquist/camillagui-backend',
        'Loopback — Windows VB-CABLE · vb-audio.com, Mac BlackHole',
        'Audio interface — the one you use now',
    ]),
    'cam_path': ('Signal path', [
        'Loopback — CamillaDSP takes the sound here. The capture device',
        'CamillaDSP — route_in → SECS correction → route_out',
        'Interface — CamillaDSP sends the sound here. The playback device',
    ]),
    'cam_folder': ('The camilladsp folder', [
        'bin · gui — unzip the downloads here',
        'configs · coeffs — be sure to make these empty folders first',
    ]),
    'cam_export': ('Exported folder → camilladsp folder', [
        'The yml in configs → camilladsp configs',
        'The wav files in coeffs → camilladsp coeffs. The paths in the config are ../coeffs/, so they open as they are',
        'in.raw — only for the SECS check. Do not copy it',
    ]),
    'cam_start': ('Start order', [
        'CamillaDSP engine — keep this window open. Closing it stops the sound',
        'GUI server — one more window opens',
        'Open this address in the browser',
    ]),
    'cam_files': ('Files tab', [
        'Upload files under Filters — all the wav files from coeffs first',
        'Upload files under Configs — then the yml',
        'A green check under Valid — if not, hover to see why',
        '★ — marks it active and loads it into the GUI',
    ]),
    'cam_devices': ('Devices tab — Windows example', [
        'Start with samplerate 48000 · chunksize 1024',
        'Capture device = loopback. channels = route_in in',
        'Playback device = interface. channels = route_out out',
        'Two devices with different clocks, so turn on Rate adjust. The AsyncSinc resampler is for Windows only',
    ]),
    'cam_order': ('Channel table at the top of the config — 5.1 example', [
        'Inputs — the order CamillaDSP receives',
        'Outputs — the order CamillaDSP sends',
    ]),
    'cam_route_in': ('route_in — when the player sends LFE 4th', [
        'Across = channels the player sends. Numbers start at 0',
        'Move only the cells in the Ls · Rs · LFE rows',
    ]),
    'cam_route_out': ('route_out — sub on output 4, surrounds on 5 · 6', [
        'out = number of interface outputs. Set the playback channels on Devices to match',
        'Move only the cells in the output 4 · 5 · 6 rows. CamillaDSP numbers are one lower',
    ]),
    'cam_check': ('GUI left panel', [
        'Apply to DSP — sends the GUI config to CamillaDSP',
        'State RUNNING means it is running',
        'Input · output meters — they move when the player plays',
        'Clipped samples — if it goes up, turn the volume down',
        'Save to file — save when it works',
    ]),
    'cam_latency': ('Latency by correction mode', [
        'Audio sync in the video player — move it earlier by the latency',
    ]),
}
assert _ko.camilla_shape(CAMILLA, CAMILLA_CAPTIONS) == _ko.camilla_shape() and set(CAMILLA_CAPTIONS) == set(_ko.CAMILLA_CAPTIONS), \
    'en_g6: CamillaDSP 탭 모양이 tutorial_text 와 다르다'

PAIRS: tuple[tuple[str, str], ...] = tuple(zip(
    _ko.all_strings(),
    _ko.all_strings(INTRO, TUTORIAL, CAPTIONS, HINT, TAB_INTRO, TABS),
)) + tuple(zip(_ko.names_strings(), _ko.names_strings(NAMES, EXTRA_TABS, NAMES_CAPTIONS))) \
    + tuple(zip(_ko.camilla_strings(), _ko.camilla_strings(CAMILLA, CAMILLA_CAPTIONS))) + _WINDOW_PAIRS
