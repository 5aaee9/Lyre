"use client";

import { FormEvent, useId, useState } from "react";
import { useRouter } from "next/navigation";
import { AlertCircle, CheckCircle2, Hash, Keyboard, Mic, Radio, Server, Settings, UserRound, Waves } from "lucide-react";
import { SettingsDialog } from "@/components/settings-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { joinRoom } from "@/lib/api";
import { useSettingsStore } from "@/lib/settings-store";

export default function Home() {
  const router = useRouter();
  const roomIdInputId = useId();
  const nicknameInputId = useId();
  const rememberInputId = useId();
  const remember = useSettingsStore((state) => state.rememberRoom);
  const storedRoomId = useSettingsStore((state) => state.roomId);
  const nickname = useSettingsStore((state) => state.nickname);
  const noise = useSettingsStore((state) => state.noise);
  const setRemember = useSettingsStore((state) => state.setRememberRoom);
  const setStoredRoomId = useSettingsStore((state) => state.setRoomId);
  const setNickname = useSettingsStore((state) => state.setNickname);
  const [roomId, setRoomId] = useState(() => (remember ? storedRoomId : "DEFAULT"));
  const [joining, setJoining] = useState(false);
  const [joinError, setJoinError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const targetRoom = roomId.trim() || "DEFAULT";
  const displayName = nickname.trim() || "Auto-assigned";
  const noiseLabel = noiseProviderLabel(noise.provider);

  async function onJoin(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (joining) {
      return;
    }
    setJoining(true);
    setJoinError(null);
    try {
      if (remember) {
        setStoredRoomId(targetRoom);
      }
      const response = await joinRoom(targetRoom, { nickname, noise });
      sessionStorage.setItem(
        "lyre.roomSession",
        JSON.stringify({ roomId: targetRoom, user: response.user, accessToken: response.access_token })
      );
      router.push(`/room/${encodeURIComponent(targetRoom)}`);
    } catch (error) {
      setJoinError(error instanceof Error ? error.message : "Failed to join room");
      setJoining(false);
    }
  }

  return (
    <section className="grid items-start gap-4 lg:grid-cols-[minmax(0,1fr)_19rem]">
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      <div className="rounded-xl border border-lyre-border bg-card">
        <div className="grid gap-6 px-4 py-5 sm:px-5 lg:px-6 lg:py-6">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div className="grid gap-3">
              <div className="flex items-center gap-2 text-sm font-medium text-lyre-soft-foreground">
                <span className="grid size-8 place-items-center rounded-lg bg-lyre-soft">
                  <Radio aria-hidden="true" className="size-4" />
                </span>
                Server relay voice
              </div>
              <div>
                <h1 className="text-2xl font-semibold tracking-tight">Join a voice room</h1>
                <p className="mt-1 max-w-2xl text-sm text-lyre-muted-foreground">
                  Pick a room, set the name people will see, and Lyre will start relay audio when you enter.
                </p>
              </div>
            </div>
            <Button aria-label="Settings" className="self-start" onClick={() => setSettingsOpen(true)} variant="outline">
              <Settings aria-hidden="true" className="size-4" />
              <span>Settings</span>
            </Button>
          </div>

          <form className="grid gap-4" onSubmit={onJoin}>
            <div className="grid gap-4 sm:grid-cols-2">
              <label className="grid gap-2 text-sm font-medium text-foreground" htmlFor={roomIdInputId}>
                <span className="flex items-center gap-2">
                  <Hash aria-hidden="true" className="size-4 text-lyre-muted-foreground" />
                  Room ID
                </span>
                <Input
                  autoComplete="off"
                  id={roomIdInputId}
                  value={roomId}
                  onChange={(event) => setRoomId(event.target.value)}
                  placeholder="DEFAULT"
                />
              </label>
              <label className="grid gap-2 text-sm font-medium text-foreground" htmlFor={nicknameInputId}>
                <span className="flex items-center gap-2">
                  <UserRound aria-hidden="true" className="size-4 text-lyre-muted-foreground" />
                  Nickname
                </span>
                <Input
                  autoComplete="nickname"
                  id={nicknameInputId}
                  value={nickname}
                  onChange={(event) => setNickname(event.target.value)}
                  placeholder="Assigned automatically if blank"
                />
              </label>
            </div>

            <div className="flex flex-col gap-3 border-t border-lyre-subtle-border pt-4 sm:flex-row sm:items-center sm:justify-between">
              <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-lyre-soft-foreground">
                <label className="flex items-center gap-2" htmlFor={rememberInputId}>
                  <Switch checked={remember} id={rememberInputId} onCheckedChange={setRemember} />
                  Remember this room
                </label>
                <span className="inline-flex items-center gap-1.5 text-lyre-muted-foreground">
                  <Keyboard aria-hidden="true" className="size-4" />
                  Enter joins
                </span>
              </div>
              <Button className="sm:min-w-36" disabled={joining} type="submit">
                <Mic aria-hidden="true" className="size-4" />
                <span>{joining ? "Joining..." : "Join voice"}</span>
              </Button>
            </div>

            {joinError ? (
              <div
                className="flex items-start gap-2 rounded-lg border border-lyre-danger-border bg-lyre-danger-bg px-3 py-2 text-sm text-lyre-danger-text"
                role="alert"
              >
                <AlertCircle aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
                <span>{joinError}</span>
              </div>
            ) : null}
          </form>
        </div>
      </div>

      <aside className="rounded-xl border border-lyre-border bg-card">
        <div className="border-b border-lyre-subtle-border px-4 py-3">
          <div className="text-sm font-semibold">Entry preview</div>
          <div className="text-xs text-lyre-muted-foreground">What Lyre will use after join</div>
        </div>
        <dl className="grid divide-y divide-lyre-subtle-border">
          <EntryRow icon={Hash} label="Room" value={targetRoom} />
          <EntryRow icon={UserRound} label="Name" value={displayName} />
          <EntryRow icon={Server} label="Audio path" value="Server relay" />
          <EntryRow icon={Waves} label="Noise" value={noiseLabel} />
        </dl>
        <div className="border-t border-lyre-subtle-border px-4 py-3">
          <span className="inline-flex items-center gap-2 rounded-full border border-lyre-success-border bg-lyre-success-bg px-2.5 py-1 text-xs font-medium text-lyre-success-text">
            <CheckCircle2 aria-hidden="true" className="size-3.5" />
            Ready for voice
          </span>
        </div>
      </aside>
    </section>
  );
}

function EntryRow({
  icon: Icon,
  label,
  value
}: {
  icon: typeof Hash;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-center gap-3 px-4 py-3 text-sm">
      <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-lyre-soft text-lyre-soft-foreground">
        <Icon aria-hidden="true" className="size-4" />
      </span>
      <div className="min-w-0">
        <dt className="text-xs font-medium text-lyre-muted-foreground">{label}</dt>
        <dd className="truncate font-medium text-foreground">{value}</dd>
      </div>
    </div>
  );
}

function noiseProviderLabel(provider: string): string {
  switch (provider) {
    case "dpdfnet":
      return "DPDFNet";
    case "deepfilternet":
      return "DeepFilterNet";
    case "rnnoise":
      return "RNNoise";
    default:
      return "Off";
  }
}
