"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
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
  const t = useTranslations("RoomDiagnostics");
  const [diagnostics, setDiagnostics] = useState<ServerMediaAudioDiagnostics | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setDiagnostics(await loadDiagnostics());
      setError(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : t("failedToLoad"));
    }
  }, [loadDiagnostics, t]);

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
        <div className="text-sm font-semibold">{t("title")}</div>
        <button className="text-xs text-lyre-accent underline underline-offset-2" onClick={() => void refresh()}>
          {t("refresh")}
        </button>
      </div>
      <div className="grid gap-4 p-4 text-sm">
        {error ? <div className="text-lyre-danger-text">{error}</div> : null}
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2">
          <dt className="text-lyre-muted-foreground">{t("peer")}</dt>
          <dd>{diagnostics?.connectionState ?? t("unavailable")}</dd>
          <dt className="text-lyre-muted-foreground">ICE</dt>
          <dd>{diagnostics?.iceConnectionState ?? t("unavailable")}</dd>
          <dt className="text-lyre-muted-foreground">{t("signaling")}</dt>
          <dd>{diagnostics?.signalingState ?? t("unavailable")}</dd>
          <dt className="text-lyre-muted-foreground">{t("audioContext")}</dt>
          <dd>{diagnostics?.audioContextState ?? t("unavailable")}</dd>
        </dl>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2">
          <dt className="text-lyre-muted-foreground">{t("packetsSent")}</dt>
          <dd>{diagnostics?.stats.packetsSent ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">{t("bytesSent")}</dt>
          <dd>{diagnostics?.stats.bytesSent ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">{t("packetsReceived")}</dt>
          <dd>{diagnostics?.stats.packetsReceived ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">{t("bytesReceived")}</dt>
          <dd>{diagnostics?.stats.bytesReceived ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">{t("packetsLost")}</dt>
          <dd>{diagnostics?.stats.packetsLost ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">{t("remoteLost")}</dt>
          <dd>{diagnostics?.stats.remotePacketsLost ?? 0}</dd>
          <dt className="text-lyre-muted-foreground">{t("audioLevel")}</dt>
          <dd>{diagnostics?.stats.audioLevel === null ? t("unavailable") : diagnostics?.stats.audioLevel.toFixed(4) ?? t("unavailable")}</dd>
          <dt className="text-lyre-muted-foreground">{t("audioEnergy")}</dt>
          <dd>{diagnostics?.stats.totalAudioEnergy === null ? t("unavailable") : diagnostics?.stats.totalAudioEnergy.toFixed(4) ?? t("unavailable")}</dd>
          <dt className="text-lyre-muted-foreground">{t("audioDuration")}</dt>
          <dd>{diagnostics?.stats.totalSamplesDuration === null ? t("unavailable") : `${diagnostics?.stats.totalSamplesDuration.toFixed(2) ?? "0.00"} s`}</dd>
          <dt className="text-lyre-muted-foreground">RTT</dt>
          <dd>{diagnostics?.stats.roundTripTimeMs === null ? t("unavailable") : `${diagnostics?.stats.roundTripTimeMs ?? 0} ms`}</dd>
        </dl>
        <dl className="grid gap-2">
          <dt className="text-lyre-muted-foreground">{t("relayParticipants")}</dt>
          <dd>{relaySourceIds.length ? relaySourceIds.join(", ") : t("none")}</dd>
          <dt className="text-lyre-muted-foreground">{t("subscribedSources")}</dt>
          <dd>{subscribedSourceIds.length ? subscribedSourceIds.join(", ") : t("none")}</dd>
          <dt className="text-lyre-muted-foreground">{t("remoteTracks")}</dt>
          <dd>{diagnostics?.remoteTrackIds.length ? diagnostics.remoteTrackIds.join(", ") : t("none")}</dd>
          <dt className="text-lyre-muted-foreground">{t("receiverTracks")}</dt>
          <dd>{diagnostics?.receiverTrackIds.length ? diagnostics.receiverTrackIds.join(", ") : t("none")}</dd>
          <dt className="text-lyre-muted-foreground">{t("trackEvents")}</dt>
          <dd>{diagnostics?.onTrackTrackIds.length ? diagnostics.onTrackTrackIds.join(", ") : t("none")}</dd>
          <dt className="text-lyre-muted-foreground">{t("rejectedTracks")}</dt>
          <dd>{diagnostics?.rejectedTrackIds.length ? diagnostics.rejectedTrackIds.join(", ") : t("none")}</dd>
          <dt className="text-lyre-muted-foreground">{t("remoteSources")}</dt>
          <dd>
            {diagnostics?.remoteSources.length
              ? diagnostics.remoteSources
                .map((source) =>
                  t("remoteSourceSummary", {
                    userId: source.userId,
                    gain: source.gain.toFixed(2),
                    muted: String(source.muted),
                    volume: source.volumePercent,
                    tracks: source.trackIds.join("/"),
                    states: source.readyStates.join("/"),
                    enabled: source.enabled.join("/")
                  })
                )
                .join("; ")
              : t("none")}
          </dd>
          <dt className="text-lyre-muted-foreground">{t("playbackError")}</dt>
          <dd>{diagnostics?.lastPlaybackError ?? t("none")}</dd>
        </dl>
      </div>
    </div>
  );
}
