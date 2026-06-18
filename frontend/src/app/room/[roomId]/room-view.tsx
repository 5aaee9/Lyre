"use client";

import { LogOut, Mic, MicOff, Radio, Settings, Share2, Users, Volume2, VolumeX } from "lucide-react";
import { SettingsDialog } from "@/components/settings-dialog";
import { Button } from "@/components/ui/button";
import type { RoomSnapshot, UserProfile } from "@/lib/api";
import type { SettingsSnapshot, UserAudioSettings } from "@/lib/settings-store";
import type { ServerMediaAudioDiagnostics } from "@/lib/server-media-audio";
import { RoomAudioDiagnostics } from "./room-audio-diagnostics";

type RoomViewProps = {
  accessToken: string | null;
  audioDiagnosticsEnabled: boolean;
  audioDiagnosticsRefreshKey: number;
  audioStarted: boolean;
  currentUser: UserProfile | null;
  link: string;
  loadAudioDiagnostics: () => Promise<ServerMediaAudioDiagnostics | null>;
  muted: boolean;
  onApplyUserAudioSettings: (userId: string, settings: Partial<UserAudioSettings>) => void | Promise<void>;
  onLeave: () => void | Promise<void>;
  onSaveSettings: (settings: SettingsSnapshot) => void | Promise<void>;
  onSettingsOpenChange: (open: boolean) => void;
  onToggleMuted: () => void;
  relaySourceIds: string[];
  room: RoomSnapshot | null;
  roomId: string;
  settingsOpen: boolean;
  speakingUserIds: Set<string>;
  status: string;
  subscribedSourceIds: string[];
  userAudio: Record<string, UserAudioSettings>;
};

const DEFAULT_USER_AUDIO_SETTINGS: UserAudioSettings = { muted: false, volumePercent: 100 };

export function RoomView({
  accessToken,
  audioDiagnosticsEnabled,
  audioDiagnosticsRefreshKey,
  audioStarted,
  currentUser,
  link,
  loadAudioDiagnostics,
  muted,
  onApplyUserAudioSettings,
  onLeave,
  onSaveSettings,
  onSettingsOpenChange,
  onToggleMuted,
  relaySourceIds,
  room,
  roomId,
  settingsOpen,
  speakingUserIds,
  status,
  subscribedSourceIds,
  userAudio
}: RoomViewProps) {
  const users = room?.users ?? [];
  const remoteCount = users.filter((user) => user.id !== currentUser?.id).length;
  const isRecovering = status.toLowerCase().includes("reconnect") || status.toLowerCase().includes("joining");
  const isProblem = status.toLowerCase().includes("failed") || status.toLowerCase().includes("error");

  return (
    <section className="grid gap-4">
      <SettingsDialog open={settingsOpen} onOpenChange={onSettingsOpenChange} onSave={onSaveSettings} />
      <div className="rounded-xl border border-[#d8ded6] bg-white">
        <div className="flex flex-col gap-4 px-4 py-4 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="truncate text-2xl font-semibold tracking-tight">{roomId}</h1>
              <span className="inline-flex items-center gap-1 rounded-full border border-[#d8ded6] bg-[#f6f8f5] px-2 py-1 text-xs font-medium text-[#334038]">
                <Users className="size-3.5" aria-hidden="true" />
                {users.length} online
              </span>
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <RoomStatusBadge isProblem={isProblem} isRecovering={isRecovering} status={status} />
              <span className="text-sm text-[#5c6a61]">
                {remoteCount === 1 ? "1 listener available" : `${remoteCount} listeners available`}
              </span>
            </div>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button aria-label="Settings" onClick={() => onSettingsOpenChange(true)} variant="outline">
              <Settings aria-hidden="true" className="size-4" />
              <span>Settings</span>
            </Button>
            <Button aria-pressed={muted} disabled={!audioStarted} onClick={onToggleMuted} variant={muted ? "outline" : "default"}>
              {muted ? <MicOff aria-hidden="true" className="size-4" /> : <Mic aria-hidden="true" className="size-4" />}
              <span>{muted ? "Unmute" : "Mute"}</span>
            </Button>
            <Button onClick={() => void onLeave()} variant="destructive">
              <LogOut aria-hidden="true" className="size-4" />
              <span>Leave</span>
            </Button>
          </div>
        </div>
        <div className="border-t border-[#edf0ec] px-4 py-3">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-2 text-xs font-medium text-[#5c6a61]">
              <Share2 aria-hidden="true" className="size-3.5" />
              Invite link
            </div>
            <div className="min-w-0 break-all rounded-lg bg-[#f6f8f5] px-3 py-2 text-sm text-[#18211c] sm:max-w-[70%]">
              {link}
            </div>
          </div>
        </div>
      </div>

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
        <div className="rounded-xl border border-[#d8ded6] bg-white">
          <div className="flex items-center justify-between gap-3 border-b border-[#d8ded6] px-4 py-3">
            <div>
              <div className="text-sm font-semibold">Voice channel</div>
              <div className="text-xs text-[#5c6a61]">Server relay audio</div>
            </div>
            <span className="rounded-full bg-[#eef3ed] px-2 py-1 text-xs font-medium text-[#334038]">
              {users.length} {users.length === 1 ? "user" : "users"}
            </span>
          </div>
          <ul className="divide-y divide-[#edf0ec]">
            {users.map((user) => {
              const audioSettings = userAudio[user.id] ?? DEFAULT_USER_AUDIO_SETTINGS;
              const isCurrentUser = user.id === currentUser?.id;
              const isSpeaking = speakingUserIds.has(user.id);
              return (
                <li className="flex flex-col gap-3 px-4 py-3 text-sm sm:flex-row sm:items-center sm:justify-between" key={user.id}>
                  <div className="flex min-w-0 items-center gap-3">
                    <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-[#eef3ed] text-sm font-semibold text-[#334038]">
                      {initialsFor(user.nickname)}
                    </span>
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="truncate font-medium text-[#18211c]">{user.nickname}</span>
                        {isCurrentUser ? (
                          <span className="rounded-full border border-[#d8ded6] px-2 py-0.5 text-xs text-[#5c6a61]">You</span>
                        ) : null}
                        {isSpeaking ? (
                          <span
                            aria-label={`${user.nickname} is speaking`}
                            className="inline-flex items-center gap-1 rounded-full bg-[#e9f5eb] px-2 py-0.5 text-xs font-medium text-[#1f5c31] ring-1 ring-[#9fd0a9]"
                          >
                            <Radio aria-hidden="true" className="size-3" />
                            Speaking
                          </span>
                        ) : null}
                        {!isCurrentUser && audioSettings.muted ? (
                          <span className="inline-flex items-center gap-1 rounded-full bg-[#f3efec] px-2 py-0.5 text-xs font-medium text-[#6c4b3c] ring-1 ring-[#dbc7bd]">
                            <VolumeX aria-hidden="true" className="size-3" />
                            Muted
                          </span>
                        ) : null}
                      </div>
                      <div className="mt-0.5 text-xs text-[#5c6a61]">{isCurrentUser ? "Local microphone" : "Remote audio"}</div>
                    </div>
                  </div>
                  {!isCurrentUser ? (
                    <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                      <Button
                        aria-pressed={audioSettings.muted}
                        onClick={() => void onApplyUserAudioSettings(user.id, { muted: !audioSettings.muted })}
                        size="sm"
                        variant="outline"
                      >
                        {audioSettings.muted ? (
                          <Volume2 aria-hidden="true" className="size-3.5" />
                        ) : (
                          <VolumeX aria-hidden="true" className="size-3.5" />
                        )}
                        <span>{audioSettings.muted ? `Unmute ${user.nickname}` : `Mute ${user.nickname}`}</span>
                      </Button>
                      <label className="flex min-w-[11rem] items-center gap-2 text-xs text-[#5c6a61]">
                        <span className="w-9 text-right tabular-nums">{audioSettings.volumePercent}%</span>
                        <input
                          aria-label={`${user.nickname} volume`}
                          className="h-8 w-28 accent-[#256141] focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
                          max={150}
                          min={0}
                          onChange={(event) =>
                            void onApplyUserAudioSettings(user.id, { volumePercent: Number(event.target.value) })
                          }
                          type="range"
                          value={audioSettings.volumePercent}
                        />
                      </label>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ul>
        </div>

        <aside className="grid content-start gap-4">
          <div className="rounded-xl border border-[#d8ded6] bg-white p-4">
            <div className="text-sm font-semibold">Relay</div>
            <dl className="mt-3 grid gap-2 text-sm">
              <div className="flex items-center justify-between gap-3">
                <dt className="text-[#5c6a61]">Session</dt>
                <dd className="font-medium text-[#18211c]">{accessToken ? "Authenticated" : "Joining"}</dd>
              </div>
              <div className="flex items-center justify-between gap-3">
                <dt className="text-[#5c6a61]">Subscribed</dt>
                <dd className="font-medium text-[#18211c]">{subscribedSourceIds.length}</dd>
              </div>
              <div className="flex items-center justify-between gap-3">
                <dt className="text-[#5c6a61]">Sources</dt>
                <dd className="font-medium text-[#18211c]">{relaySourceIds.length}</dd>
              </div>
            </dl>
          </div>
          {audioDiagnosticsEnabled ? (
            <RoomAudioDiagnostics
              loadDiagnostics={loadAudioDiagnostics}
              relaySourceIds={relaySourceIds}
              refreshKey={audioDiagnosticsRefreshKey}
              subscribedSourceIds={subscribedSourceIds}
            />
          ) : null}
        </aside>
      </div>
    </section>
  );
}

function RoomStatusBadge({
  isProblem,
  isRecovering,
  status
}: {
  isProblem: boolean;
  isRecovering: boolean;
  status: string;
}) {
  const tone = isProblem
    ? "border-[#efc2bc] bg-[#fff1ef] text-[#8b2e22]"
    : isRecovering
      ? "border-[#d9cfaa] bg-[#fff8df] text-[#66540c]"
      : "border-[#b9d8bd] bg-[#eef8ef] text-[#255c33]";
  const dot = isProblem ? "bg-[#b83b2e]" : isRecovering ? "bg-[#8d7212]" : "bg-[#2f8f46]";
  return (
    <span className={`inline-flex items-center gap-2 rounded-full border px-2.5 py-1 text-xs font-medium ${tone}`}>
      <span className={`size-2 rounded-full ${dot}`} aria-hidden="true" />
      <span>{status}</span>
    </span>
  );
}

function initialsFor(name: string) {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("") || "?";
}
