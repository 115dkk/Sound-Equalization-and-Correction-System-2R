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

local function channel_settings(label, track)
  log(label .. " guid=" .. reaper.GetTrackGUID(track))
  for _, key in ipairs({"I_NCHAN", "C_MAINSEND_NCH", "C_MAINSEND_OFFS", "B_MAINSEND"}) do
    log(label .. " " .. key .. "=" .. reaper.GetMediaTrackInfo_Value(track, key))
  end
end
local function dump(label)
  log(label .. " tracks=" .. reaper.CountTracks(project))
  local master = reaper.GetMasterTrack(project)
  channel_settings("MASTER", master)
  for i = 0, reaper.CountTracks(project) - 1 do
    local track = reaper.GetTrack(project, i)
    local _, name = reaper.GetTrackName(track)
    log(string.format("TRACK %d %q guid=%s", i + 1, name, reaper.GetTrackGUID(track)))
    channel_settings("TRACK_CHANNELS", track)
    channel_settings("PARENT_CHANNELS", reaper.GetParentTrack(track) or master)
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
  -- Only this guarded scratch project is changed, never a user's master.
  local master = reaper.GetMasterTrack(project)
  assert(reaper.SetMediaTrackInfo_Value(master, "I_NCHAN", channels))
  assert(reaper.GetMediaTrackInfo_Value(master, "I_NCHAN") == channels)
  assert(reaper.SetMediaTrackInfo_Value(master, "D_VOL", 1))
  assert(reaper.SetMediaTrackInfo_Value(master, "D_PAN", 0))
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
  -- Assert the user's full path: path tracks -> bus -> folder -> master.
  -- Bus and individual-path stems below are diagnostics, not the assertion.
  -- The first CI render ended at 9.386s instead of 9.386666...s (32 frames
  -- short), consistent with millisecond truncation in the render action. Use
  -- explicit whole-second custom bounds beyond the source, not project length.
  -- Rust still rejects any missing expected frame and checks the extra silence.
  local render_end = math.ceil(frames / 48000) + 1
  local render = {RENDER_SETTINGS = 0, RENDER_BOUNDSFLAG = 0,
    RENDER_STARTPOS = 0, RENDER_ENDPOS = render_end,
    RENDER_CHANNELS = channels, RENDER_SRATE = 48000,
    RENDER_TAILFLAG = 0, RENDER_TAILMS = 0, RENDER_ADDTOPROJ = 0,
    RENDER_DITHER = 16, RENDER_NORMALIZE = 0}
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_FILE", root, true))
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_PATTERN", "render", true))
  -- Sink bytes: 'evaw', 0x20, 0, 0, 0. In the pinned REAPER the WAV depth
  -- selector uses 0x20 for 32-bit FP, 0x21 for 32-bit PCM (not the reverse).
  -- Rust independently requires IEEE_FLOAT/EXTENSIBLE_FLOAT in the actual WAV.
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_FORMAT", "ZXZhdyAAAAA=", true))
  assert(reaper.GetSetProjectInfo_String(project, "RENDER_FORMAT2", "", true))
  local function render_settings(label, check)
    log(label .. " expected_frames=" .. frames .. " requested_end=" .. render_end)
    for key, value in pairs(render) do
      local actual = reaper.GetSetProjectInfo(project, key, 0, false)
      log(string.format("%s=%.17g requested=%.17g", key, actual, value))
      if check then assert(actual == value, "render setting was not retained: " .. key) end
    end
  end
  local function render_file(pattern, settings, count, track)
    if track then reaper.SetOnlyTrackSelected(track) end
    render.RENDER_SETTINGS = settings
    render.RENDER_CHANNELS = count
    assert(reaper.GetSetProjectInfo_String(project, "RENDER_PATTERN", pattern, true))
    for key, value in pairs(render) do reaper.GetSetProjectInfo(project, key, value, true) end
    render_settings(pattern .. " BEFORE_RENDER", true)
    local _, targets = reaper.GetSetProjectInfo_String(project, "RENDER_TARGETS", "", false)
    log("RENDER_TARGETS " .. targets)
    reaper.Main_OnCommand(42230, 0) -- Render using current settings, auto-close render window.
    render_settings(pattern .. " AFTER_RENDER", false)
    assert(reaper.file_exists(root .. "/" .. pattern .. ".wav"), "render output is missing: " .. pattern)
    log(pattern .. " RENDER_COMPLETE")
  end
  render_file("render", 0, channels) -- Master mix is the only audio assertion.
  render_file("stem_bus", 2 | 4, channels, bus)
  local center, left
  for i = 0, reaper.CountTracks(project) - 1 do
    local track = reaper.GetTrack(project, i)
    local _, name = reaper.GetTrackName(track)
    if name == "C → C" then center = track end
    if name == "L → L" then left = track end
  end
  -- mimo_iir has only L and SW1 outputs, so it has no C -> C path.
  local diagnostic_path = assert(center or left, "diagnostic direct path is missing")
  local _, diagnostic_name = reaper.GetTrackName(diagnostic_path)
  log("DIAGNOSTIC_PATH " .. diagnostic_name)
  render_file("stem_path", 2 | 4, reaper.GetMediaTrackInfo_Value(diagnostic_path, "I_NCHAN"), diagnostic_path)
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
