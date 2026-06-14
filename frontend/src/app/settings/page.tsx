"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { parseNoiseProvider, type NoiseCancellationConfig } from "@/lib/api";
import { readNickname, readNoiseConfig, writeNickname, writeNoiseConfig } from "@/lib/storage";

export default function SettingsPage() {
  const [nickname, setNickname] = useState(() => readNickname());
  const [noise, setNoise] = useState<NoiseCancellationConfig>(() => readNoiseConfig());
  const [saved, setSaved] = useState(false);

  function save() {
    writeNickname(nickname);
    writeNoiseConfig(noise);
    setSaved(true);
  }

  return (
    <section className="grid max-w-xl gap-5">
      <div>
        <h1 className="text-2xl font-semibold">Settings</h1>
        <p className="mt-1 text-sm text-[#5c6a61]">Saved locally in this browser.</p>
      </div>
      <label className="grid gap-2 text-sm font-medium">
        Nickname
        <Input value={nickname} onChange={(event) => setNickname(event.target.value)} />
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
      <label className="grid gap-2 text-sm font-medium">
        Intensity
        <Input
          max={1}
          min={0}
          step={0.05}
          type="number"
          value={noise.intensity}
          onChange={(event) =>
            setNoise((current) => ({
              ...current,
              intensity: Number(event.target.value)
            }))
          }
        />
      </label>
      <label className="grid gap-2 text-sm font-medium">
        Voice activity threshold
        <Input
          max={1}
          min={0}
          step={0.05}
          type="number"
          value={noise.voice_activity_threshold}
          onChange={(event) =>
            setNoise((current) => ({
              ...current,
              voice_activity_threshold: Number(event.target.value)
            }))
          }
        />
      </label>
      <Button onClick={save}>Save</Button>
      {saved ? <p className="text-sm text-[#1f6f50]">Saved</p> : null}
    </section>
  );
}
