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
    <div className="rounded-md border border-[#d8ded6] bg-white">
      <div className="flex items-center justify-between gap-3 border-b border-[#d8ded6] px-4 py-3">
        <div className="text-sm font-semibold">Audio diagnostics</div>
        <button className="text-xs text-[#256141] underline underline-offset-2" onClick={() => void refresh()}>
          Refresh
        </button>
      </div>
      <div className="grid gap-4 p-4 text-sm">
        {error ? <div className="text-red-700">{error}</div> : null}
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2">
          <dt className="text-[#5c6a61]">Peer</dt>
          <dd>{diagnostics?.connectionState ?? "unavailable"}</dd>
          <dt className="text-[#5c6a61]">ICE</dt>
          <dd>{diagnostics?.iceConnectionState ?? "unavailable"}</dd>
          <dt className="text-[#5c6a61]">Signaling</dt>
          <dd>{diagnostics?.signalingState ?? "unavailable"}</dd>
          <dt className="text-[#5c6a61]">Audio context</dt>
          <dd>{diagnostics?.audioContextState ?? "unavailable"}</dd>
        </dl>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2">
          <dt className="text-[#5c6a61]">Packets sent</dt>
          <dd>{diagnostics?.stats.packetsSent ?? 0}</dd>
          <dt className="text-[#5c6a61]">Bytes sent</dt>
          <dd>{diagnostics?.stats.bytesSent ?? 0}</dd>
          <dt className="text-[#5c6a61]">Packets received</dt>
          <dd>{diagnostics?.stats.packetsReceived ?? 0}</dd>
          <dt className="text-[#5c6a61]">Bytes received</dt>
          <dd>{diagnostics?.stats.bytesReceived ?? 0}</dd>
          <dt className="text-[#5c6a61]">Packets lost</dt>
          <dd>{diagnostics?.stats.packetsLost ?? 0}</dd>
          <dt className="text-[#5c6a61]">Remote lost</dt>
          <dd>{diagnostics?.stats.remotePacketsLost ?? 0}</dd>
          <dt className="text-[#5c6a61]">RTT</dt>
          <dd>{diagnostics?.stats.roundTripTimeMs === null ? "unavailable" : `${diagnostics?.stats.roundTripTimeMs ?? 0} ms`}</dd>
        </dl>
        <dl className="grid gap-2">
          <dt className="text-[#5c6a61]">Relay participants</dt>
          <dd>{relaySourceIds.length ? relaySourceIds.join(", ") : "none"}</dd>
          <dt className="text-[#5c6a61]">Subscribed sources</dt>
          <dd>{subscribedSourceIds.length ? subscribedSourceIds.join(", ") : "none"}</dd>
          <dt className="text-[#5c6a61]">Remote tracks</dt>
          <dd>{diagnostics?.remoteTrackIds.length ? diagnostics.remoteTrackIds.join(", ") : "none"}</dd>
          <dt className="text-[#5c6a61]">Receiver tracks</dt>
          <dd>{diagnostics?.receiverTrackIds.length ? diagnostics.receiverTrackIds.join(", ") : "none"}</dd>
          <dt className="text-[#5c6a61]">Track events</dt>
          <dd>{diagnostics?.onTrackTrackIds.length ? diagnostics.onTrackTrackIds.join(", ") : "none"}</dd>
          <dt className="text-[#5c6a61]">Rejected tracks</dt>
          <dd>{diagnostics?.rejectedTrackIds.length ? diagnostics.rejectedTrackIds.join(", ") : "none"}</dd>
          <dt className="text-[#5c6a61]">Playback error</dt>
          <dd>{diagnostics?.lastPlaybackError ?? "none"}</dd>
        </dl>
      </div>
    </div>
  );
}
