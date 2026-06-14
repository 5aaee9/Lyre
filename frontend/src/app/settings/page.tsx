"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { parseNoiseProvider } from "@/lib/api";
import { readNickname, readNoiseConfig, writeNickname, writeNoiseConfig } from "@/lib/storage";

export default function SettingsPage() {
  const [nickname, setNickname] = useState(() => readNickname());
  const [provider, setProvider] = useState(() => readNoiseConfig().provider);
  const [saved, setSaved] = useState(false);

  function save() {
    writeNickname(nickname);
    writeNoiseConfig({ provider, intensity: 0.5, voice_activity_threshold: 0.35 });
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
        <Select value={provider} onChange={(event) => setProvider(parseNoiseProvider(event.target.value))}>
          <option value="off">Off</option>
          <option value="rnnoise">RNNoise</option>
          <option value="deepfilternet">DeepFilterNet</option>
        </Select>
      </label>
      <Button onClick={save}>Save</Button>
      {saved ? <p className="text-sm text-[#1f6f50]">Saved</p> : null}
    </section>
  );
}
