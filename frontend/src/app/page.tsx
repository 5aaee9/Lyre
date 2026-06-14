"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { joinRoom, parseNoiseProvider, type NoiseCancellationConfig } from "@/lib/api";
import { readNickname, readNoiseConfig, readRememberRoom, readRoomId, writeNickname, writeNoiseConfig, writeRememberRoom, writeRoomId } from "@/lib/storage";

export default function Home() {
  const router = useRouter();
  const [remember, setRemember] = useState(() => readRememberRoom());
  const [roomId, setRoomId] = useState(() => (readRememberRoom() ? readRoomId() : "DEFAULT"));
  const [nickname, setNickname] = useState(() => readNickname());
  const [noise, setNoise] = useState<NoiseCancellationConfig>(() => readNoiseConfig());
  const [joining, setJoining] = useState(false);

  async function onJoin() {
    setJoining(true);
    const targetRoom = roomId.trim() || "DEFAULT";
    if (remember) {
      writeRoomId(targetRoom);
    }
    writeRememberRoom(remember);
    writeNickname(nickname);
    writeNoiseConfig(noise);
    const response = await joinRoom(targetRoom, { nickname, noise });
    sessionStorage.setItem("lyre.currentUser", JSON.stringify(response.user));
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
              setNoise((current) => ({
                ...current,
                provider: parseNoiseProvider(event.target.value)
              }))
            }
          >
            <option value="off">Off</option>
            <option value="rnnoise">RNNoise</option>
            <option value="deepfilternet">DeepFilterNet</option>
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
