export type VoiceActivityDetectorOptions = {
  sampleIntervalMs?: number;
  rmsThreshold?: number;
  speakingStartMs?: number;
  speakingStopMs?: number;
};

const DEFAULT_SAMPLE_INTERVAL_MS = 40;
const DEFAULT_RMS_THRESHOLD = 0.02;
const DEFAULT_SPEAKING_START_MS = 100;
const DEFAULT_SPEAKING_STOP_MS = 650;
const ANALYSER_FFT_SIZE = 1024;

export class VoiceActivityDetector {
  private readonly sampleIntervalMs: number;
  private readonly rmsThreshold: number;
  private readonly speakingStartMs: number;
  private readonly speakingStopMs: number;
  private audioContext?: AudioContext;
  private source?: MediaStreamAudioSourceNode;
  private analyser?: AnalyserNode;
  private samples?: Float32Array<ArrayBuffer>;
  private interval?: number;
  private speaking = false;
  private aboveThresholdMs = 0;
  private belowThresholdMs = 0;

  constructor(
    private readonly stream: MediaStream,
    private readonly onSpeakingChange: (speaking: boolean) => void,
    options: VoiceActivityDetectorOptions = {}
  ) {
    this.sampleIntervalMs = options.sampleIntervalMs ?? DEFAULT_SAMPLE_INTERVAL_MS;
    this.rmsThreshold = options.rmsThreshold ?? DEFAULT_RMS_THRESHOLD;
    this.speakingStartMs = options.speakingStartMs ?? DEFAULT_SPEAKING_START_MS;
    this.speakingStopMs = options.speakingStopMs ?? DEFAULT_SPEAKING_STOP_MS;
  }

  start(): void {
    if (this.interval !== undefined) {
      return;
    }
    const audioContext = new AudioContext();
    const source = audioContext.createMediaStreamSource(this.stream);
    const analyser = audioContext.createAnalyser();
    analyser.fftSize = ANALYSER_FFT_SIZE;
    source.connect(analyser);
    this.audioContext = audioContext;
    this.source = source;
    this.analyser = analyser;
    this.samples = new Float32Array(analyser.fftSize);
    this.interval = window.setInterval(() => this.sample(), this.sampleIntervalMs);
  }

  stop(): void {
    if (this.interval !== undefined) {
      window.clearInterval(this.interval);
      this.interval = undefined;
    }
    this.source?.disconnect();
    this.analyser?.disconnect();
    void this.audioContext?.close();
    this.audioContext = undefined;
    this.source = undefined;
    this.analyser = undefined;
    this.samples = undefined;
    this.speaking = false;
    this.aboveThresholdMs = 0;
    this.belowThresholdMs = 0;
  }

  private sample(): void {
    if (!this.analyser || !this.samples) {
      return;
    }
    this.analyser.getFloatTimeDomainData(this.samples);
    const rms = calculateRms(this.samples);
    if (rms >= this.rmsThreshold) {
      this.aboveThresholdMs += this.sampleIntervalMs;
      this.belowThresholdMs = 0;
      if (!this.speaking && this.aboveThresholdMs >= this.speakingStartMs) {
        this.speaking = true;
        this.onSpeakingChange(true);
      }
      return;
    }
    this.belowThresholdMs += this.sampleIntervalMs;
    this.aboveThresholdMs = 0;
    if (this.speaking && this.belowThresholdMs >= this.speakingStopMs) {
      this.speaking = false;
      this.onSpeakingChange(false);
    }
  }
}

export function calculateRms(samples: Float32Array<ArrayBufferLike>): number {
  let sum = 0;
  for (const sample of samples) {
    sum += sample * sample;
  }
  return Math.sqrt(sum / samples.length);
}
