"use client";

import { useEffect, useState } from "react";
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
import {
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { parseNoiseProvider } from "@/lib/api";
import { readSettingsSnapshot, useSettingsStore, type SettingsSnapshot } from "@/lib/settings-store";

type SettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave?: (settings: SettingsSnapshot) => void | Promise<void>;
};

const DEFAULT_DEVICE_VALUE = "default";

export function SettingsDialog({ open, onOpenChange, onSave }: SettingsDialogProps) {
  const nickname = useSettingsStore((state) => state.nickname);
  const audioDiagnosticsEnabled = useSettingsStore((state) => state.audioDiagnosticsEnabled);
  const noise = useSettingsStore((state) => state.noise);
  const audioProcessing = useSettingsStore((state) => state.audioProcessing);
  const audioDevices = useSettingsStore((state) => state.audioDevices);
  const setNickname = useSettingsStore((state) => state.setNickname);
  const setAudioDiagnosticsEnabled = useSettingsStore((state) => state.setAudioDiagnosticsEnabled);
  const setNoise = useSettingsStore((state) => state.setNoise);
  const setAudioProcessing = useSettingsStore((state) => state.setAudioProcessing);
  const setAudioDevices = useSettingsStore((state) => state.setAudioDevices);
  const [mediaDevices, setMediaDevices] = useState<MediaDeviceInfo[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    let cancelled = false;
    void navigator.mediaDevices?.enumerateDevices?.()
      .then((devices) => {
        if (!cancelled) {
          setMediaDevices(devices);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setMediaDevices([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

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
                value={noise.provider}
                onValueChange={(value) =>
                  setNoise({
                    ...noise,
                    provider: parseNoiseProvider(value)
                  })
                }
              >
                <SelectTrigger
                  aria-label="Server Noise Cancelling"
                  className="col-span-3 w-full"
                  id="settings-server-noise"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="off">Off</SelectItem>
                  <SelectItem value="rnnoise">RNNoise</SelectItem>
                  <SelectItem value="deepfilternet">DeepFilterNet</SelectItem>
                  <SelectItem value="dpdfnet">DPDFNet</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {noise.provider === "dpdfnet" ? (
              <div className="grid grid-cols-4 items-center gap-4">
                <label className="text-right text-sm font-medium" htmlFor="settings-dpdfnet-model">
                  Model
                </label>
                <Select
                  value={noise.dpdfnet.model}
                  onValueChange={(value) =>
                    setNoise({
                      ...noise,
                      dpdfnet: {
                        model: value
                      }
                    })
                  }
                >
                  <SelectTrigger
                    aria-label="DPDFNet model"
                    className="col-span-3 w-full"
                    id="settings-dpdfnet-model"
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="baseline">baseline</SelectItem>
                    <SelectItem value="dpdfnet2">dpdfnet2</SelectItem>
                    <SelectItem value="dpdfnet4">dpdfnet4</SelectItem>
                    <SelectItem value="dpdfnet8">dpdfnet8</SelectItem>
                    <SelectItem value="dpdfnet2_48khz_hr">dpdfnet2_48khz_hr</SelectItem>
                    <SelectItem value="dpdfnet8_48khz_hr">dpdfnet8_48khz_hr</SelectItem>
                  </SelectContent>
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
              <label className="text-right text-sm font-medium" htmlFor="settings-microphone">
                Microphone
              </label>
              <Select
                value={audioDevices.inputDeviceId || DEFAULT_DEVICE_VALUE}
                onValueChange={(value) =>
                  setAudioDevices({
                    ...audioDevices,
                    inputDeviceId: value === DEFAULT_DEVICE_VALUE ? "" : value
                  })
                }
              >
                <SelectTrigger
                  aria-label="Microphone"
                  className="col-span-3 w-full"
                  id="settings-microphone"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={DEFAULT_DEVICE_VALUE}>Default microphone</SelectItem>
                  {mediaDevices.filter((device) => device.kind === "audioinput").map((device) => (
                    <SelectItem key={device.deviceId} value={device.deviceId}>
                      {device.label || "Microphone"}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-speaker">
                Speaker
              </label>
              <Select
                value={audioDevices.outputDeviceId || DEFAULT_DEVICE_VALUE}
                onValueChange={(value) =>
                  setAudioDevices({
                    ...audioDevices,
                    outputDeviceId: value === DEFAULT_DEVICE_VALUE ? "" : value
                  })
                }
              >
                <SelectTrigger
                  aria-label="Speaker"
                  className="col-span-3 w-full"
                  id="settings-speaker"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={DEFAULT_DEVICE_VALUE}>Default speaker</SelectItem>
                  {mediaDevices.filter((device) => device.kind === "audiooutput").map((device) => (
                    <SelectItem key={device.deviceId} value={device.deviceId}>
                      {device.label || "Speaker"}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid grid-cols-4 items-center gap-4">
              <label className="text-right text-sm font-medium" htmlFor="settings-audio-diagnostics">
                Diagnostics
              </label>
              <Switch
                aria-label="Audio diagnostics"
                className="col-span-3"
                id="settings-audio-diagnostics"
                checked={audioDiagnosticsEnabled}
                onCheckedChange={setAudioDiagnosticsEnabled}
              />
            </div>
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
