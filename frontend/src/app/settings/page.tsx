"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { parseNoiseProvider } from "@/lib/api";
import { useSettingsStore } from "@/lib/settings-store";

export default function SettingsPage() {
  const nickname = useSettingsStore((state) => state.nickname);
  const noise = useSettingsStore((state) => state.noise);
  const audioProcessing = useSettingsStore((state) => state.audioProcessing);
  const setNickname = useSettingsStore((state) => state.setNickname);
  const setNoise = useSettingsStore((state) => state.setNoise);
  const setAudioProcessing = useSettingsStore((state) => state.setAudioProcessing);
  const [saved, setSaved] = useState(false);

  function save() {
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
            setNoise({
              ...noise,
              provider: parseNoiseProvider(event.target.value)
            })
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
            setNoise({
              ...noise,
              intensity: Number(event.target.value)
            })
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
            setNoise({
              ...noise,
              voice_activity_threshold: Number(event.target.value)
            })
          }
        />
      </label>
      <label className="flex items-center gap-2 text-sm">
        <Switch
          checked={audioProcessing.echoCancellation}
          onChange={(event) =>
            setAudioProcessing({
              ...audioProcessing,
              echoCancellation: event.target.checked
            })
          }
        />
        Echo cancellation
      </label>
      <label className="flex items-center gap-2 text-sm">
        <Switch
          checked={audioProcessing.autoGainControl}
          onChange={(event) =>
            setAudioProcessing({
              ...audioProcessing,
              autoGainControl: event.target.checked
            })
          }
        />
        Auto gain control
      </label>
      <Button onClick={save}>Save</Button>
      {saved ? <p className="text-sm text-[#1f6f50]">Saved</p> : null}
    </section>
  );
}
