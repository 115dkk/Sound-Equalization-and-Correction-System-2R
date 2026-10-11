@echo off
rem SECS 2.x wizard with real REW. Start REW first with the -api option.
rem The exe name carries the version (SECS 2.0 Alpha1.exe ...), so pick it by pattern.
for %%f in ("%~dp0SECS\SECS 2*.exe") do start "" "%%f"
