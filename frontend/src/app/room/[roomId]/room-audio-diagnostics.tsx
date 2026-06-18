"use client";

import { useCallback, useEffect, useState } from "react";
import type { ServerMediaAudioDiagnostics } from "@/lib/server-media-audio";

type RoomAudioDiagnosticsProps = {
  relaySourceIds: string[];
  refreshKey: number;
  subscribedSourceIds: string[];
  loadDiagnostics: () => Promise<ServerMediaAudioDiagnostics | null>;
};

const REFRESH_INTERVAL_MS = 1_000;

export function RoomAudioDiagnostics({
  relaySourceIds,
  refreshKey,
  subscribedSourceIds,
  loadDiagnostics
}: RoomAudioDiagnosticsProps) {
  const [diagnostics, setDiagnostics] = useState<ServerMediaAudioDiagnostics | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setDiagnostics(await loadDiagnostics());
      setError(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Failed to load audio diagnostics");
    }
  }, [loadDiagnostics]);

  useEffect(() => {
    const timeout = window.setTimeout(() => void refresh(), 0);
    const interval = window.setInterval(() => void refresh(), REFRESH_INTERVAL_MS);
    return () => {
      window.clearTimeout(timeout);
      window.clearInterval(interval);
    };
  }, [refresh, refreshKey]);

  return (
    <div className="rounded-md border border-lyre-border bg-card">
      <div className="flex items-center justify-between gap-3 border-b border-lyre-border px-4 py-3">
        <div className="text-sm font-semibold">Audio diagnostics</div>
        <button className="text-xs text-lyre-accent underline underline-offset-2" onClick={() => void refresh()}>
          Refresh
        </button>
      </div>
      <div className="grid gap-4 p-4 text-sm">
        {error ? <div className="text-lyre-danger-text">{error}</div> : null}
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2">
          <dt className="text-lyre-muted-foreground">Peer</dt>
          <dd>{diagnostics?.connectionState ?? "unavailable"}</dd>
          <dt className="text-lyre-muted-foreground">ICE</dt>
          <dd>{diagnostics?.iceConnectionState ?? "unavailable"}</dd>
          <dt className="text-lyre-muted-foreground">Signaling</dt>
          <dd>{diagnostics?.signalingState ?? "unavailable"}</dd>
          <dt className="text-lyre-muted-foreground">Audio context</dt>
          <dd>{diagnostics?.audioContextState ?? "unavailable"}</dd>
        </dl>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2">
          <dt className="text-lyre-muted-foreground">Packets sent</dt>
          <dd>{diagnostics?.stats.packetsSent ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">Bytes sent</dt>
          <dd>{diagnostics?.stats.bytesSent ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">Packets received</dt>
          <dd>{diagnostics?.stats.packetsReceived ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">Bytes received</dt>
          <dd>{diagnostics?.stats.bytesReceived ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">Packets lost</dt>
          <dd>{diagnostics?.stats.packetsLost ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">Remote lost</dt>
          <dd>{diagnostics?.stats.remotePacketsLost ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">Audio level</dt>
          <dd>{diagnostics?.stats.audioLevel === null ? "unavailable" : diagnostics?.stats.audioLevel.toFixed(4) ?? "unavailable"}</dd>
          <dt className="text-lyre-muted-foreground">Audio energy</dt>
          <dd>{diagnostics?.stats.totalAudioEnergy === null ? "unavailable" : diagnostics?.stats.totalAudioEnergy.toFixed(4) ?? "unavailable"}</dd>
          <dt className="text-lyre-muted-foreground">Audio duration</dt>
          <dd>{diagnostics?.stats.totalSamplesDuration === null ? "unavailable" : `${diagnostics?.stats.totalSamplesDuration.toFixed(2) ?? "0.00"} s`}</dd>
          <dt className="text-lyre-muted-foreground">RTT</dt>
          <dd>{diagnostics?.stats.roundTripTimeMs === null ? "unavailable" : `${diagnostics?.stats.roundTripTimeMs ?? 0} ms`}</dd>
        </dl>
        <dl className="grid gap-2">
          <dt className="text-lyre-muted-foreground">Relay participants</dt>
          <dd>{relaySourceIds.length ? relaySourceIds.join(", ") : "none"}</dd>
          <dt className="text-lyre-muted-foreground">Subscribed sources</dt>
          <dd>{subscribedSourceIds.length ? subscribedSourceIds.join(", ") : "none"}</dd>
          <dt className="text-lyre-muted-foreground">Remote tracks</dt>
          <dd>{diagnostics?.remoteTrackIds.length ? diagnostics.remoteTrackIds.join(", ") : "none"}</dd>
          <dt className="text-lyre-muted-foreground">Receiver tracks</dt>
          <dd>{diagnostics?.receiverTrackIds.length ? diagnostics.receiverTrackIds.join(", ") : "none"}</dd>
          <dt className="text-lyre-muted-foreground">Track events</dt>
          <dd>{diagnostics?.onTrackTrackIds.length ? diagnostics.onTrackTrackIds.join(", ") : "none"}</dd>
          <dt className="text-lyre-muted-foreground">Rejected tracks</dt>
          <dd>{diagnostics?.rejectedTrackIds.length ? diagnostics.rejectedTrackIds.join(", ") : "none"}</dd>
          <dt className="text-lyre-muted-foreground">Remote sources</dt>
          <dd>
            {diagnostics?.remoteSources.length
              ? diagnostics.remoteSources
                .map((source) =>
                  `${source.userId}: gain ${source.gain.toFixed(2)}, muted ${source.muted}, volume ${source.volumePercent}%, tracks ${source.trackIds.join("/")}, states ${source.readyStates.join("/")}, enabled ${source.enabled.join("/")}`
                )
                .join("; ")
              : "none"}
          </dd>
          <dt className="text-lyre-muted-foreground">Playback error</dt>
          <dd>{diagnostics?.lastPlaybackError ?? "none"}</dd>
        </dl>
      </div>
    </div>
  );
}
