-- Run only in a separate REAPER instance whose resource directory is the
-- disposable SECS_REAPER_TEST_ROOT directory. See README.md in this folder.
local root = assert(os.getenv("SECS_REAPER_TEST_ROOT"), "SECS_REAPER_TEST_ROOT is required")
local bundle = assert(os.getenv("SECS_REAPER_EXAMPLE"), "SECS_REAPER_EXAMPLE is required")
local function normalized(path) return path:gsub("\\", "/"):gsub("/+$", ""):lower() end
assert(normalized(reaper.GetResourcePath()) == normalized(root), "refusing to use the normal REAPER resource directory")
assert(reaper.file_exists(root .. "/SECS_REAPER_TEST_ONLY"), "scratch-directory marker missing")
local project, filename = reaper.EnumProjects(-1, "")
assert(normalized(filename) == normalized(root .. "/empty.rpp"), "not the scratch project")
assert(not reaper.EnumProjects(1, ""), "other project tabs are open")
assert(reaper.CountTracks(project) == 0, "scratch project is not empty")
local out = assert(io.open(root .. "/dump.txt", "w"))
local function log(text) out:write(tostring(text), "\n"); out:flush() end
local show_message = reaper.ShowMessageBox
local messages = {}
reaper.ShowMessageBox = function(text)
  messages[#messages + 1] = text
  log("MESSAGE " .. text)
  return 0
end
local function dump(label)
  log(label .. " tracks=" .. reaper.CountTracks(project))
  for i = 0, reaper.CountTracks(project) - 1 do
    local track = reaper.GetTrack(project, i)
    local _, name = reaper.GetTrackName(track)
    log(string.format("TRACK %d %q", i + 1, name))
    for _, key in ipairs({"I_NCHAN", "I_FOLDERDEPTH", "B_MAINSEND", "D_VOL", "D_PANLAW", "B_MUTE"}) do
      log(key .. "=" .. reaper.GetMediaTrackInfo_Value(track, key))
    end
    for s = 0, reaper.GetTrackNumSends(track, 0) - 1 do
      local destination = reaper.GetTrackSendInfo_Value(track, 0, s, "P_DESTTRACK")
      log("SEND " .. s .. " to=" .. reaper.GetMediaTrackInfo_Value(destination, "IP_TRACKNUMBER"))
      for _, key in ipairs({"I_SRCCHAN", "I_DSTCHAN", "I_SENDMODE", "I_MIDIFLAGS", "D_VOL", "D_PANLAW"}) do
        log(key .. "=" .. reaper.GetTrackSendInfo_Value(track, 0, s, key))
      end
    end
    for fx = 0, reaper.TrackFX_GetCount(track) - 1 do
      local _, name = reaper.TrackFX_GetFXName(track, fx, "")
      log("FX " .. fx .. " " .. name)
      for p = 0, reaper.TrackFX_GetNumParams(track, fx) - 1 do
        local _, name = reaper.TrackFX_GetParamName(track, fx, p, "")
        local _, value = reaper.TrackFX_GetFormattedParamValue(track, fx, p, "")
        log(string.format("PARAM %d %s raw=%.17g formatted=%s", p, name,
          reaper.TrackFX_GetParamNormalized(track, fx, p), value))
      end
      for _, key in ipairs({"ITEM0", "ITEM1", "pdc", "vst_chunk"}) do
        local found, value = reaper.TrackFX_GetNamedConfigParm(track, fx, key)
        log(key .. " found=" .. tostring(found) .. " " .. value)
      end
      for direction = 0, 1 do
        for pin = 0, 1 do
          local low, high = reaper.TrackFX_GetPinMappings(track, fx, direction, pin)
          log(string.format("PIN %d %d %d %d", direction, pin, low, high))
        end
      end
    end
    local ok, chunk = reaper.GetTrackStateChunk(track, "", false)
    assert(ok); log(chunk)
  end
end
local ok, detail = xpcall(function()
  log("REAPER " .. reaper.GetAppVersion())
  reaper.InsertTrackAtIndex(0, false)
  local source = reaper.GetTrack(project, 0)
  reaper.GetSetMediaTrackInfo_String(source, "P_NAME", "Scratch source", true)
  reaper.SetOnlyTrackSelected(source)
  dofile(bundle .. "/secs2r_reaper.lua")
  dump("FIRST")
  assert(reaper.CountTracks(project) == 5, "two-path fixture did not create five tracks")
  assert(reaper.GetMediaTrackInfo_Value(source, "B_MAINSEND") == 0)
  local _, first_name = reaper.GetTrackName(reaper.GetTrack(project, 1))
  assert(first_name == "SECS 2R 거실")
  local first_guid = reaper.GetTrackGUID(reaper.GetTrack(project, 1))
  reaper.SetOnlyTrackSelected(source)
  dofile(bundle .. "/secs2r_reaper.lua")
  dump("SECOND")
  assert(reaper.CountTracks(project) == 5, "second run duplicated tracks")
  assert(first_guid ~= reaper.GetTrackGUID(reaper.GetTrack(project, 1)), "folder not replaced")
  assert(reaper.GetTrackNumSends(source, 0) == 2, "old source sends remain")
  assert(#messages == 2 and messages[2]:find("Correction tracks are ready", 1, true))
end, debug.traceback)
log(ok and "PASS" or "FAIL " .. tostring(detail))
reaper.ShowMessageBox = show_message
out:close()
-- Official Main_openProject noprompt prefix discards ONLY this guarded scratch
-- project. Reload the unchanged empty project before quitting, so no save dialog.
reaper.Main_openProject("noprompt:" .. root .. "/empty.rpp")
reaper.Main_OnCommand(40004, 0) -- File: Quit REAPER
