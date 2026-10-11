@echo off
rem SECS 2.x wizard with sample data (no REW, no sound).
for %%f in ("%~dp0SECS\SECS 2*.exe") do start "" "%%f" --fake-rew
