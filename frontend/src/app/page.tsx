"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { joinRoom, parseNoiseProvider } from "@/lib/api";
import { useSettingsStore } from "@/lib/settings-store";

export default function Home() {
  const router = useRouter();
  const remember = useSettingsStore((state) => state.rememberRoom);
  const storedRoomId = useSettingsStore((state) => state.roomId);
  const nickname = useSettingsStore((state) => state.nickname);
  const noise = useSettingsStore((state) => state.noise);
  const setRemember = useSettingsStore((state) => state.setRememberRoom);
  const setStoredRoomId = useSettingsStore((state) => state.setRoomId);
  const setNickname = useSettingsStore((state) => state.setNickname);
  const setNoise = useSettingsStore((state) => state.setNoise);
  const [roomId, setRoomId] = useState(() => (remember ? storedRoomId : "DEFAULT"));
  const [joining, setJoining] = useState(false);

  async function onJoin() {
    setJoining(true);
    const targetRoom = roomId.trim() || "DEFAULT";
    if (remember) {
      setStoredRoomId(targetRoom);
    }
    const response = await joinRoom(targetRoom, { nickname, noise });
    sessionStorage.setItem(
      "lyre.roomSession",
      JSON.stringify({ roomId: targetRoom, user: response.user, accessToken: response.access_token })
    );
    router.push(`/room/${encodeURIComponent(targetRoom)}`);
  }

  return (
    <section className="grid gap-5">
      <div>
        <h1 className="text-2xl font-semibold">Join a room</h1>
        <p className="mt-1 text-sm text-[#5c6a61]">Enter a room and start a voice session.</p>
      </div>
      <div className="grid max-w-xl gap-4 rounded-md border border-[#d8ded6] bg-white p-4">
        <label className="grid gap-2 text-sm font-medium">
          Room ID
          <Input value={roomId} onChange={(event) => setRoomId(event.target.value)} />
        </label>
        <label className="grid gap-2 text-sm font-medium">
          Nickname
          <Input value={nickname} onChange={(event) => setNickname(event.target.value)} placeholder="Assigned automatically if blank" />
        </label>
        <label className="grid gap-2 text-sm font-medium">
          Noise cancellation
          <Select
            value={noise.provider}
            onChange={(event) =>
              setNoise({
                ...noise,
                provider: parseNoiseProvider(event.target.value)
              })
            }
          >
            <option value="off">Off</option>
          <option value="rnnoise">RNNoise</option>
          <option value="deepfilternet">DeepFilterNet</option>
          <option value="dpdfnet">DPDFNet</option>
        </Select>
      </label>
        <label className="flex items-center gap-2 text-sm">
          <Switch checked={remember} onChange={(event) => setRemember(event.target.checked)} />
          Remember Room ID
        </label>
        <Button disabled={joining} onClick={onJoin}>
          {joining ? "Joining..." : "Join"}
        </Button>
      </div>
    </section>
  );
}
