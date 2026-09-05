import { useState } from "react";
import { transport, useTransport } from "../transport/transport";
import { openContextFiles, openSketchStore } from "../sketches/openSketch";
import {
  updateMarkerSource,
  useTimelineMarkers,
  useTimelineError,
  validMarkerName,
  resolveTimeline,
  timelineSettings,
  TIMELINE_FILE,
  useTimelineSettings,
  type TimelineMarker,
  type TimelineSettings,
} from "./timeline";

export function TimelinePanel() {
  const { t } = useTransport();
  const markers = useTimelineMarkers();
  const error = useTimelineError();
  const settings = useTimelineSettings();
  const { loopIn, loopOut, looping } = settings;
  const [markerName, setMarkerName] = useState("");

  const save = (
    nextMarkers: TimelineMarker[],
    nextSettings: TimelineSettings = settings,
    rename?: { from: string; to: string },
  ) => saveTimeline(nextMarkers, nextSettings, rename);

  const addMarker = () => {
    const name = markerName.trim();
    if (!validMarkerName(name)) return;
    save([...markers.filter((marker) => marker.name !== name), { name, time: t }]);
    setMarkerName("");
  };

  return (
    <fieldset className="timeline timeline--panel" disabled={!!error}>
      {error && (
        <p role="alert">
          {error} Fix timeline.lua to resume panel editing. Showing the last valid markers.
        </p>
      )}
      <div className="timeline-controls">
        <TimelineLength />
        {(["loopIn", "loopOut"] as const).map((bound) => {
          const label = bound === "loopIn" ? "Loop in" : "Loop out";
          const ref = bound === "loopIn" ? "loopInMarker" : "loopOutMarker";
          return (
            <div className="timeline-bound" role="group" aria-label={label} key={bound}>
              <span>{label}</span>
              <select
                aria-label={`${label} marker`}
                value={settings[ref] ?? ""}
                onChange={(event) =>
                  save(markers, { ...settings, [ref]: event.target.value || undefined })
                }
              >
                <option value="">Time</option>
                {markers.map((marker) => (
                  <option key={marker.name} value={marker.name}>
                    {marker.name}
                  </option>
                ))}
              </select>
              <input
                type="number"
                min={0}
                step="any"
                aria-label={`${label} seconds`}
                value={settings[bound]}
                disabled={!!settings[ref]}
                onChange={(event) => {
                  const value = event.target.valueAsNumber;
                  if (Number.isFinite(value) && value >= 0)
                    save(markers, { ...settings, [bound]: value });
                }}
              />
              <button
                type="button"
                onClick={() => save(markers, { ...settings, [bound]: t, [ref]: undefined })}
              >
                Set {bound === "loopIn" ? "in" : "out"}
              </button>
            </div>
          );
        })}
        <label>
          <input
            type="checkbox"
            checked={looping}
            onChange={(event) => save(markers, { ...settings, looping: event.target.checked })}
          />
          Loop range
        </label>
        {loopOut <= loopIn && (
          <span role="alert">Loop out must be after loop in to play a loop.</span>
        )}
        <form
          onSubmit={(event) => {
            event.preventDefault();
            addMarker();
          }}
        >
          <input
            value={markerName}
            aria-label="Marker name"
            placeholder="marker_name"
            pattern="[A-Za-z_][A-Za-z0-9_]*"
            onChange={(event) => setMarkerName(event.target.value)}
          />
          <button type="submit">Add marker at {t.toFixed(1)}s</button>
        </form>
      </div>
      {markers.length > 0 && (
        <div className="timeline-markers" aria-label="Timeline markers">
          {markers.map((marker) => (
            <MarkerRow
              key={`${marker.name}:${marker.time}`}
              marker={marker}
              onSave={(next) =>
                save(
                  [
                    ...markers.filter(
                      (item) => item.name !== marker.name && item.name !== next.name,
                    ),
                    next,
                  ],
                  {
                    ...settings,
                    loopInMarker:
                      settings.loopInMarker === marker.name ? next.name : settings.loopInMarker,
                    loopOutMarker:
                      settings.loopOutMarker === marker.name ? next.name : settings.loopOutMarker,
                  },
                  { from: marker.name, to: next.name },
                )
              }
              onDelete={() => save(markers.filter((item) => item.name !== marker.name))}
            />
          ))}
        </div>
      )}
    </fieldset>
  );
}

function MarkerRow({
  marker,
  onSave,
  onDelete,
}: {
  marker: TimelineMarker;
  onSave: (marker: TimelineMarker) => void;
  onDelete: () => void;
}) {
  const [name, setName] = useState(marker.name);
  const [time, setTime] = useState(marker.time);
  const valid = validMarkerName(name) && Number.isFinite(time);
  return (
    <div className="timeline-marker">
      <button
        type="button"
        onClick={() => {
          transport.setPlaying(false);
          transport.seek(marker.time);
        }}
        aria-label={`Go to ${marker.name}`}
      >
        Go to
      </button>
      <input
        aria-label="Marker name"
        value={name}
        onChange={(event) => setName(event.target.value)}
      />
      <input
        type="number"
        min={0}
        step={1 / 60}
        aria-label={`${marker.name} time`}
        value={time}
        onChange={(event) => setTime(Math.max(0, Number(event.target.value) || 0))}
      />
      <button type="button" disabled={!valid} onClick={() => onSave({ name, time })}>
        Save
      </button>
      <button
        type="button"
        onClick={() => void navigator.clipboard?.writeText(`markers.${marker.name}`)}
      >
        Copy Lua
      </button>
      <button type="button" onClick={onDelete} aria-label={`Delete ${marker.name}`}>
        Delete
      </button>
    </div>
  );
}

export function saveTimeline(
  markers: TimelineMarker[],
  settings: TimelineSettings,
  rename?: { from: string; to: string },
) {
  if (timelineSettings.error()) return false;
  const source = openContextFiles(openSketchStore.state()).find(
    (file) => file.name === TIMELINE_FILE,
  )!.source;
  const next = resolveTimeline(settings, markers);
  openSketchStore.editFile(TIMELINE_FILE, updateMarkerSource(source, markers, next, rename));
  return true;
}

export function TimelineLength() {
  const { end } = useTimelineSettings();
  const error = useTimelineError();
  return (
    <label>
      Length
      <input
        disabled={!!error}
        title={error}
        key={end}
        type="number"
        min={1}
        step="any"
        defaultValue={end}
        aria-label="Timeline length in seconds"
        onBlur={(event) => {
          const value = event.currentTarget.valueAsNumber;
          if (Number.isFinite(value) && value >= 1) {
            saveTimeline(timelineSettings.markers(), {
              ...timelineSettings.get(),
              end: value,
            });
          } else event.currentTarget.value = String(end);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
      s
    </label>
  );
}
