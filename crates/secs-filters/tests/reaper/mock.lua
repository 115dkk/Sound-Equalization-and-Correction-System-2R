-- REAPER-free control-flow tests, not a substitute for the real host harness.
local script = assert(arg[1])
local tracks, selected, messages, refresh, begins, ends, failure
local marker = "P_EXT:SECS2R_CONVOLUTION"
local names = {"Wet", "Dry", "Pre-verb", "Max FFT size", "Width", "Pan", "ZL mode", "LL mode", ":wet"}
local function track(name)
  return {P_NAME = name, I_NCHAN = 2, I_FOLDERDEPTH = 0, B_MAINSEND = 1,
    sends = {}, params = {}, config = {}, guid = tostring({})}
end
local function reset()
  tracks = {track("Source"), track("Unrelated")}
  selected, messages, refresh, begins, ends, failure = tracks[1], {}, 0, 0, 0, nil
end
reaper = {}
function reaper.GetSelectedTrack() return selected end
function reaper.GetSetMediaTrackInfo_String(t, key, value, write)
  if write then t[key] = value end
  return true, t[key] or ""
end
function reaper.GetMediaTrackInfo_Value(t, key)
  if key == "IP_TRACKNUMBER" then
    for i, value in ipairs(tracks) do if t == value then return i end end
  end
  return t[key] or 0
end
function reaper.SetMediaTrackInfo_Value(t, key, value) t[key] = value; return true end
function reaper.GetTrackGUID(t) return t.guid end
function reaper.CountTracks() return #tracks end
function reaper.GetTrack(_, index) return tracks[index + 1] end
function reaper.InsertTrackAtIndex(index, defaults)
  assert(defaults == false)
  table.insert(tracks, index + 1, track("New"))
end
function reaper.DeleteTrack(t)
  for _, from in ipairs(tracks) do
    for i = #from.sends, 1, -1 do
      if from.sends[i].to == t then table.remove(from.sends, i) end
    end
  end
  for i, value in ipairs(tracks) do
    if t == value then table.remove(tracks, i); return end
  end
  error("deleted an unknown track")
end
function reaper.CreateTrackSend(from, to)
  if failure == "send" then return -1 end
  from.sends[#from.sends + 1] = {to = to}
  return #from.sends - 1
end
function reaper.SetTrackSendInfo_Value(t, _, index, key, value)
  t.sends[index + 1][key] = value; return true
end
function reaper.TrackFX_AddByName(t, name, rec, instantiate)
  assert(name == "ReaVerb (Cockos)" and not rec and instantiate == -1)
  if failure == "fx" then return -1 end
  t.fx = name
  t.config.ITEM0 = "NORM 0" -- a user preset must not leak through
  return 0
end
function reaper.TrackFX_SetPresetByIndex(t, _, index)
  assert(index == -2); t.config = {}; return true
end
function reaper.TrackFX_GetNamedConfigParm(t, _, key)
  return t.config[key] ~= nil, t.config[key] or ""
end
function reaper.TrackFX_SetNamedConfigParm(t, _, key, value)
  if failure == "config" then return false end
  t.config[key] = value; return true
end
function reaper.TrackFX_GetNumParams() return #names end
function reaper.TrackFX_GetParamName(_, _, index)
  if failure == "parameter" and index == 6 then return true, "unknown" end
  return true, names[index + 1]
end
function reaper.TrackFX_SetParamNormalized(t, _, index, value)
  t.params[names[index + 1]] = value; return true
end
function reaper.TrackFX_GetParamNormalized(t, _, index) return t.params[names[index + 1]] end
function reaper.TrackFX_GetParamFromIdent(_, _, name) assert(name == ":wet"); return 8 end
function reaper.TrackFX_SetEnabled(t, _, enabled) t.enabled = enabled end
function reaper.TrackFX_SetPinMappings(t, _, output, pin, low, high)
  assert(high == 0 and low == (pin == 0 and 1 or 0))
  t["pin" .. output .. pin] = low; return true
end
function reaper.file_exists() return failure ~= "missing" end
function reaper.ShowMessageBox(text) messages[#messages + 1] = text end
function reaper.ShowConsoleMsg() end
function reaper.Undo_BeginBlock() begins = begins + 1 end
function reaper.Undo_EndBlock() ends = ends + 1 end
function reaper.PreventUIRefresh(delta) refresh = refresh + delta; assert(refresh >= 0) end
function reaper.TrackList_AdjustWindows() end
function reaper.UpdateArrange() end
local function run()
  dofile(script)
  assert(refresh == 0 and begins == ends, "unbalanced UI or undo block")
end
local function verify()
  assert(#tracks == 6 and #selected.sends == 2)
  assert(selected.B_MAINSEND == 0 and selected.I_NCHAN == 2)
  local folder, first, second, bus = tracks[2], tracks[3], tracks[4], tracks[5]
  assert(folder.P_NAME == "SECS 2R 거실" and folder.I_FOLDERDEPTH == 1)
  assert(folder.I_NCHAN == 4 and folder.B_MUTE == 0 and folder.B_MAINSEND == 1)
  assert(bus.I_FOLDERDEPTH == -1 and bus.I_NCHAN == 4 and bus.B_MAINSEND == 1)
  assert(tracks[6].P_NAME == "Unrelated")
  for _, t in ipairs({folder, first, second, bus}) do
    assert(t[marker] == selected.guid and t.D_VOL == 1 and t.D_PANLAW == 1)
  end
  for i, t in ipairs({first, second}) do
    local input = selected.sends[i]
    local output = t.sends[1]
    assert(input.to == t and input.I_SRCCHAN == (i == 1 and 1025 or 1024))
    assert(input.I_DSTCHAN == 1024 and input.I_MIDIFLAGS == 31)
    assert(output.to == bus and output.I_SRCCHAN == 1024)
    assert(output.I_DSTCHAN == (i == 1 and 1026 or 1024))
    assert(input.I_SENDMODE == 3 and output.I_SENDMODE == 3)
    assert(input.D_VOL == 1 and output.D_VOL == 1)
    assert(t.B_MAINSEND == 0 and t.enabled)
    assert(t.params.Wet == 1 and t.params.Dry == 0 and t.params["ZL mode"] == 1)
    assert(t.params["Pre-verb"] == 0 and t.params.Width == 1 and t.params.Pan == 0.5)
    assert(t.params[":wet"] == 1 and t.config.DONE == "" and not t.config.ITEM1)
    assert(t.config.ITEM0:match('^FILELDR ".*" 60$'))
  end
end

reset(); selected = nil; run()
assert(#tracks == 2 and #messages == 1 and begins == 0)
reset(); failure = "missing"; run()
assert(#tracks == 2 and begins == 0 and selected.B_MAINSEND == 1)
reset(); selected.I_FOLDERDEPTH = 1; run()
assert(#tracks == 2 and begins == 0)
reset(); run(); verify()
local old_folder = tracks[2]
old_folder.P_NAME = "Renamed by the user"
run(); verify()
assert(tracks[2] ~= old_folder and begins == 2)
-- Another source gets its own set without deleting this source's correction.
selected = tracks[6]; run()
assert(#tracks == 10 and #selected.sends == 2)
-- A failure never removes an earlier working set.
for _, mode in ipairs({"fx", "config", "parameter", "send"}) do
  reset(); run(); verify()
  local folder = tracks[2]
  failure = mode; run(); verify()
  assert(tracks[2] == folder)
  reset(); selected.I_NCHAN = 8; failure = mode; run()
  assert(#tracks == 2 and #selected.sends == 0)
  assert(selected.I_NCHAN == 8 and selected.B_MAINSEND == 1)
end
-- Do not delete a user's track inserted into the generated folder.
reset(); run()
table.insert(tracks, 3, track("User's audio"))
local before = begins
run()
assert(#tracks == 7 and begins == before and tracks[3].P_NAME == "User's audio")
-- Do not let a generated bus become the source on a second run.
reset(); run(); selected = tracks[5]; before = begins; run()
assert(#tracks == 6 and begins == before)
print("REAPER mock: routing, replacement, ownership, errors and undo/UI cleanup passed")
