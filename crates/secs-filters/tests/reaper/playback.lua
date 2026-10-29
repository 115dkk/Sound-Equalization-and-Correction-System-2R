-- Real-host CI harness. No extensions and no changes outside the guarded case.
-- API: https://www.reaper.fm/sdk/reascript/reascripthelp.html
-- Render action: https://forum.cockos.com/archive/index.php/t-289325.html
local root = assert(os.getenv("SECS_REAPER_TEST_ROOT"))
local function normalized(path) return path:gsub("\\", "/"):gsub("/+$", "") end
local config = root .. "/config"
assert(normalized(reaper.GetResourcePath()) == normalized(config), "not the isolated resource directory")
assert(reaper.file_exists(config .. "/SECS_REAPER_TEST_ONLY"), "scratch marker missing")
local project, filename = reaper.EnumProjects(-1, "")
assert(normalized(filename) == normalized(config .. "/source.rpp"), "not the scratch source project")
assert(not reaper.EnumProjects(1, ""), "unexpected project tabs")
local out = assert(io.open(root .. "/dump.txt", "w"))
local function log(text) out:write(tostring(text), "\n"); out:flush() end
local messages = {}
local original_message = reaper.ShowMessageBox
reaper.ShowMessageBox = function(text)
  messages[#messages + 1] = text
  log("MESSAGE " .. text)
  return 0
end
local original_console = reaper.ShowConsoleMsg
reaper.ShowConsoleMsg = function(text) log("CONSOLE " .. text) end

local function dump(label)
  log(label .. " tracks=" .. reaper.CountTracks(project))
  for i = 0, reaper.CountTracks(project) - 1 do
    local track = reaper.GetTrack(project, i)
    local _, name = reaper.GetTrackName(track)
    log(string.format("TRACK %d %q guid=%s", i + 1, name, reaper.GetTrackGUID(track)))
    for _, key in ipairs({"I_NCHAN", "I_FOLDERDEPTH", "B_MAINSEND", "D_VOL", "D_PANLAW", "B_MUTE"}) do
      log(key .. "=" .. reaper.GetMediaTrackInfo_Value(track, key))
    end
    for s = 0, reaper.GetTrackNumSends(track, 0) - 1 do
      local to = reaper.GetTrackSendInfo_Value(track, 0, s, "P_DESTTRACK")
      log("SEND " .. s .. " to=" .. reaper.GetMediaTrackInfo_Value(to, "IP_TRACKNUMBER"))
      for _, key in ipairs({"I_SRCCHAN", "I_DSTCHAN", "I_SENDMODE", "I_MIDIFLAGS", "D_VOL", "D_PANLAW"}) do
        log(key .. "=" .. reaper.GetTrackSendInfo_Value(track, 0, s, key))
      end
    end
    for fx = 0, reaper.TrackFX_GetCount(track) - 1 do
      local _, fx_name = reaper.TrackFX_GetFXName(track, fx, "")
      log("FX " .. fx .. " " .. fx_name)
      local zl = false
      for p = 0, reaper.TrackFX_GetNumParams(track, fx) - 1 do
        local _, param = reaper.TrackFX_GetParamName(track, fx, p, "")
        local _, formatted = reaper.TrackFX_GetFormattedParamValue(track, fx, p, "")
        local value = reaper.TrackFX_GetParamNormalized(track, fx, p)
        log(string.format("PARAM %d %s normalized=%.17g formatted=%s", p, param, value, formatted))
        if param == "ZL mode" then zl = value == 1 end
      end
      assert(zl, "ReaVerb zero-latency mode is off")
      for _, key in ipairs({"ITEM0", "ITEM1", "pdc", "vst_chunk"}) do
        local found, value = reaper.TrackFX_GetNamedConfigParm(track, fx, key)
        log(key .. " found=" .. tostring(found) .. " " .. value)
        if key == "pdc" and found then assert(tonumber(value) == 0, "ReaVerb reports added latency") end
      end
      for direction = 0, 1 do
        for pin = 0, 1 do
          local low, high = reaper.TrackFX_GetPinMappings(track, fx, direction, pin)
          log(string.format("PIN %d %d %d %d", direction, pin, low, high))
        end
      end
    end
    local read, chunk = reaper.GetTrackStateChunk(track, "", false)
    assert(read); log(chunk)
  end
end

local ok, detail = xpcall(function()
  log("REAPER " .. reaper.GetAppVersion())
  local inputs = assert(tonumber(os.getenv("SECS_REAPER_INPUTS")))
  local channels = assert(tonumber(os.getenv("SECS_REAPER_OUTPUTS")))
  local paths = assert(tonumber(os.getenv("SECS_REAPER_PATHS")))
  local frames = assert(tonumber(os.getenv("SECS_REAPER_FRAMES")))
  assert(reaper.CountTracks(project) == 1, "source project must contain one track")
  local source = reaper.GetTrack(project, 0)
  assert(reaper.CountTrackMediaItems(source) == 1, "source impulse item missing")
  local item = reaper.GetTrackMediaItem(source, 0)
  local take = assert(reaper.GetActiveTake(item))
  assert(reaper.GetMediaSourceNumChannels(reaper.GetMediaItemTake_Source(take)) == inputs)
  reaper.SetMediaTrackInfo_Value(source, "D_PANLAW", 1)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN", 0)
  reaper.SetMediaItemInfo_Value(item, "D_FADEINLEN_AUTO", -1)
  reaper.SetMediaItemInfo_Value(item, "D_FADEOUTLEN_AUTO", -1)
  reaper.GetSetProjectInfo(project, "PROJECT_SRATE", 48000, true)
  reaper.GetSetProjectInfo(project, "PROJECT_SRATE_USE", 1, true)
  local function build(label)
    reaper.SetOnlyTrackSelected(source)
    dofile(root .. "/bundle/secs2r_reaper.lua")
    dump(label)
    assert(messages[#messages] and messages[#messages]:find("Correction tracks are ready", 1, true), "bundle script did not succeed")
    assert(reaper.CountTracks(project) == paths + 3, "wrong track count or duplicates")
    assert(reaper.GetTrackNumSends(source, 0) == paths, "source send count differs")
    assert(reaper.GetMediaTrackInfo_Value(source, "B_MAINSEND") == 0)
    local folder = reaper.GetTrack(project, 1)
    local _, name = reaper.GetTrackName(folder)
    assert(name == "SECS 2R CI 보정", "design name was not preserved")
    local bus = reaper.GetTrack(project, paths + 2)
    assert(reaper.GetMediaTrackInfo_Value(bus, "I_NCHAN") == channels)
    assert(reaper.GetMediaTrackInfo_Value(bus, "B_MAINSEND") == 1)
    return folder, bus
  end
  local folder, bus = build("FIRST")
  local first_guid = reaper.GetTrackGUID(folder)
  reaper.SetOnlyTrackSelected(bus)
  -- Selected-track stems, preserving all track channels (not the stereo master).
  local render = {RENDER_SETTINGS = 2 | 4, RENDER_BOUNDSFLAG = 0,
    RENDER_STARTPOS = 0, RENDER_ENDPOS = frames / 48000,
    RENDER_CHANNELS = channels, RENDER_SRATE = 48000,
    RENDER_TAILFLAG = 0, RENDER_TAILMS = 0, RENDER_ADDTOPROJ = 0,
    RENDER_DITHER = 16, RENDER_NORMALIZE = 0}
  for key, value in pairs(render) do reaper.GetSetProjectInfo(project, key, value, true) end
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_FILE", root, true))
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_PATTERN", "render", true))
  -- Sink bytes: 'evaw', 0x20, 0, 0, 0. In the pinned REAPER the WAV depth
  -- selector uses 0x20 for 32-bit FP, 0x21 for 32-bit PCM (not the reverse).
  -- Rust independently requires IEEE_FLOAT/EXTENSIBLE_FLOAT in the actual WAV.
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_FORMAT", "ZXZhdyAAAAA=", true))
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_FORMAT2", "", true))
  local _, targets = reaper.GetSetProjectInfo_String(project, "RENDER_TARGETS", "", false)
  log("RENDER_TARGETS " .. targets)
  reaper.Main_OnCommand(42230, 0) -- Render using current settings, auto-close render window.
  assert(reaper.file_exists(root .. "/render.wav"), "render output is missing")
  log("RENDER_COMPLETE")
  local replacement = build("SECOND")
  assert(reaper.GetTrackGUID(replacement) ~= first_guid, "previous folder was not replaced")
  assert(#messages == 2, "unexpected bundle messages")
end, debug.traceback)
log(ok and "PASS" or "FAIL " .. tostring(detail))
local result = assert(io.open(root .. "/result.txt", "w"))
result:write(ok and "PASS\n" or "FAIL\n" .. tostring(detail) .. "\n")
result:close()
reaper.ShowMessageBox = original_message
reaper.ShowConsoleMsg = original_console
out:close()
-- Discard only this guarded test project; leave no unsaved project to prompt.
reaper.Main_openProject("noprompt:" .. config .. "/empty.rpp")
reaper.Main_OnCommand(40004, 0)
