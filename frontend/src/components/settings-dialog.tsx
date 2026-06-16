"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { parseNoiseProvider } from "@/lib/api";
import { readSettingsSnapshot, useSettingsStore, type SettingsSnapshot } from "@/lib/settings-store";

type SettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave?: (settings: SettingsSnapshot) => void | Promise<void>;
};

export function SettingsDialog({ open, onOpenChange, onSave }: SettingsDialogProps) {
  const nickname = useSettingsStore((state) => state.nickname);
  const noise = useSettingsStore((state) => state.noise);
  const audioProcessing = useSettingsStore((state) => state.audioProcessing);
  const setNickname = useSettingsStore((state) => state.setNickname);
  const setNoise = useSettingsStore((state) => state.setNoise);
  const setAudioProcessing = useSettingsStore((state) => state.setAudioProcessing);
  const [saving, setSaving] = useState(false);

  async function save() {
    setSaving(true);
    try {
      await onSave?.(readSettingsSnapshot());
      onOpenChange(false);
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Saved locally in this browser.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-5 py-4">
          <div className="grid grid-cols-4 items-center gap-4">
            <label className="text-right text-sm font-medium" htmlFor="settings-nickname">
              Nickname
            </label>
            <Input
              className="col-span-3"
              id="settings-nickname"
              value={nickname}
              onChange={(event) => setNickname(event.target.value)}
            />
          </div>
          <div className="grid gap-4 border-t border-neutral-200 pt-4">
            <div className="text-sm font-medium">Server Noise Cancelling</div>
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-server-noise">
                Provider
              </label>
              <Select
                aria-label="Server Noise Cancelling"
                className="col-span-3"
                id="settings-server-noise"
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
            </div>
            {noise.provider === "dpdfnet" ? (
              <div className="grid grid-cols-4 items-center gap-4">
                <label className="text-right text-sm font-medium" htmlFor="settings-dpdfnet-model">
                  Model
                </label>
                <Select
                  aria-label="DPDFNet model"
                  className="col-span-3"
                  id="settings-dpdfnet-model"
                  value={noise.dpdfnet.model}
                  onChange={(event) =>
                    setNoise({
                      ...noise,
                      dpdfnet: {
                        model: event.target.value
                      }
                    })
                  }
                >
                  <option value="baseline">baseline</option>
                  <option value="dpdfnet2">dpdfnet2</option>
                  <option value="dpdfnet4">dpdfnet4</option>
                  <option value="dpdfnet8">dpdfnet8</option>
                  <option value="dpdfnet2_48khz_hr">dpdfnet2_48khz_hr</option>
                  <option value="dpdfnet8_48khz_hr">dpdfnet8_48khz_hr</option>
                </Select>
              </div>
            ) : null}
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-intensity">
                Intensity
              </label>
              <Input
                className="col-span-3"
                id="settings-intensity"
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
            </div>
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-vad">
                VAD
              </label>
              <Input
                aria-label="Voice activity threshold"
                className="col-span-3"
                id="settings-vad"
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
            </div>
          </div>
          <div className="grid gap-4 border-t border-neutral-200 pt-4">
            <div className="text-sm font-medium">Browser Audio Processing</div>
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-echo-cancellation">
                Echo
              </label>
              <Switch
                aria-label="Echo cancellation"
                className="col-span-3"
                id="settings-echo-cancellation"
                checked={audioProcessing.echoCancellation}
                onCheckedChange={(checked) =>
                  setAudioProcessing({
                    ...audioProcessing,
                    echoCancellation: checked
                  })
                }
              />
            </div>
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-auto-gain-control">
                Gain
              </label>
              <Switch
                aria-label="Auto gain control"
                className="col-span-3"
                id="settings-auto-gain-control"
                checked={audioProcessing.autoGainControl}
                onCheckedChange={(checked) =>
                  setAudioProcessing({
                    ...audioProcessing,
                    autoGainControl: checked
                  })
                }
              />
            </div>
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-browser-noise">
                Suppression
              </label>
              <Switch
                aria-label="Browser noise suppression"
                className="col-span-3"
                id="settings-browser-noise"
                checked={audioProcessing.noiseSuppression}
                onCheckedChange={(checked) =>
                  setAudioProcessing({
                    ...audioProcessing,
                    noiseSuppression: checked
                  })
                }
              />
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button disabled={saving} onClick={save}>
            {saving ? "Saving..." : "Save"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
