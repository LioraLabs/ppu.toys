import { useMemo, useState } from "react";
import { transport, useTransport } from "../transport/transport";
import { openContextFiles, openSketchStore, useOpenSketch } from "../sketches/openSketch";
import {
  markerSource,
  timelineMarkers,
  timelineSettings,
  TIMELINE_FILE,
  useTimelineSettings,
  type TimelineMarker,
  type TimelineSettings,
} from "./timeline";

export function TimelinePanel() {
  const { t } = useTransport();
  const sketch = useOpenSketch();
  const files = openContextFiles(sketch);
  const markers = useMemo(() => timelineMarkers(files), [files]);
  const settings = useTimelineSettings();
  const { end, loopIn, loopOut, looping } = settings;
  const [markerName, setMarkerName] = useState("");

  const save = (nextMarkers: TimelineMarker[], nextSettings: TimelineSettings = settings) => {
    timelineSettings.set(nextSettings);
    const existed = files.some((file) => file.name === TIMELINE_FILE);
    openSketchStore.editFile(TIMELINE_FILE, markerSource(nextMarkers, nextSettings));
    if (!existed) openSketchStore.moveFile(openContextFiles(openSketchStore.state()).length - 1, 1);
  };

  const addMarker = () => {
    const name = markerName.trim();
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) return;
    save([...markers.filter((marker) => marker.name !== name), { name, time: t }]);
    setMarkerName("");
  };

  return (
    <div className="timeline timeline--panel">
      <div className="timeline-controls">
        <label>
          Length
          <input
            type="number"
            min={1}
            step={1}
            value={end}
            aria-label="Timeline length in seconds"
            onChange={(event) =>
              save(markers, { ...settings, end: Math.max(1, Number(event.target.value) || 1) })
            }
          />
          s
        </label>
        <button
          type="button"
          onClick={() => save(markers, { ...settings, loopIn: Math.min(t, loopOut) })}
        >
          Set in · {loopIn.toFixed(1)}s
        </button>
        <button
          type="button"
          onClick={() => save(markers, { ...settings, loopOut: Math.max(t, loopIn + 1 / 60) })}
        >
          Set out · {loopOut.toFixed(1)}s
        </button>
        <label>
          <input
            type="checkbox"
            checked={looping}
            onChange={(event) => save(markers, { ...settings, looping: event.target.checked })}
          />
          Loop range
        </label>
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
              key={marker.name}
              marker={marker}
              onSave={(next) =>
                save([
                  ...markers.filter(
                    (item) => item.name !== marker.name && item.name !== next.name,
                  ),
                  next,
                ])
              }
              onDelete={() => save(markers.filter((item) => item.name !== marker.name))}
            />
          ))}
        </div>
      )}
    </div>
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
  const valid = /^[A-Za-z_][A-Za-z0-9_]*$/.test(name);
  return (
    <div className="timeline-marker">
      <button type="button" onClick={() => transport.seek(marker.time)} aria-label={`Go to ${marker.name}`}>
        ◆
      </button>
      <input aria-label="Marker name" value={name} onChange={(event) => setName(event.target.value)} />
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
      <button type="button" onClick={() => void navigator.clipboard?.writeText(`markers.${marker.name}`)}>
        Copy Lua
      </button>
      <button type="button" onClick={onDelete} aria-label={`Delete ${marker.name}`}>
        Delete
      </button>
    </div>
  );
}
