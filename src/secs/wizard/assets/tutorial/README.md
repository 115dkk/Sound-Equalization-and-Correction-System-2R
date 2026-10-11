# 도움말 튜토리얼 그림

도움말(F1) 따라 하기 탭이 쓰는 화면 그림 `<key>_<ko|en>.png` 와 `MANIFEST.json`(키 → 파일·자르기·강조 사각형, 없는 키는 `missing`)이다. 키 목록과 강조 번호의 뜻은 `문서/워커 과제/도움말 튜토리얼 계약 2026-09-24.md` 1절. 손으로 고치지 말고 화면·문구가 바뀌면 다시 뽑는다: `.venv-secs\Scripts\python.exe tools\make_tutorial_shots.py`(한·영 전부, 약 1~2 분, 창은 화면에 뜨지 않음 — 일부만: `--lang en --keys s1_rew,s8_files`). 크기 예산: 파일당 ≤ 150 KB(목표 120 KB), 폭 ≤ 900 px, 폴더 전체 ≤ 6 MB — `tests/test_tutorial_assets.py` 가 지킨다. exe 에는 `SECS.spec` 의 datas 로 들어간다.

- 샘플은 실측 1점(2026-09-24): 도구는 샘플 모드 그대로(`wizard.new_project(fake=True)` — 배치 '단일 · 청취 위치 1점') 위치 1 만 잰다. 그래서 `s5_after1`·`s5_done` 은 같은 상태(모든 위치 완료)다.
- 강조 하나가 떨어진 두 구역을 가리킬 수 있다(`s6_curtain` ① = 왼쪽·오른쪽 커튼). MANIFEST 의 `marks` 는 번호마다 둘러싼 사각형 하나.
- F1 「mdat 불러오기」 탭 그림 4장(`mdat_names` · `mdat_dialog` · `mdat_files` · `mdat_channels`, 계약 그림 27개 밖)은 `tools\make_mdat_help_shots.py` 가 만든다(한·영, 창 하나는 진짜 `MdatAssignDialog` 를 잡고 나머지는 QPainter). `make_tutorial_shots.py` 를 다시 돌려도 MANIFEST 의 이 키들은 그대로 남는다.
- F1 내보내기 「CamillaDSP」 탭 그림 12장(`cam_*`, 10-10)은 `tools\make_camilla_help_shots.py` 가 만든다(한·영, 모두 QPainter — CamillaGUI 4.1 화면을 단순화해 그림). 순서표 그림(`cam_order`)은 내보내기 코드의 실제 머리 주석이라 `export_camilladsp.howto_lines` 를 바꾸면 다시 뽑는다.
