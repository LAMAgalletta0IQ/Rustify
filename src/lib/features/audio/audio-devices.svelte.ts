import * as api from "../../api";
import type { AudioDevice, AudioStatus } from "../../types";

class AudioDevicesStore {
  devices = $state<AudioDevice[]>([]);
  status = $state<AudioStatus | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  #subscribers = 0;
  #timer: number | null = null;
  #request: Promise<void> | null = null;

  refresh() {
    if (this.#request) return this.#request;
    this.loading = true;
    this.error = null;
    this.#request = Promise.all([api.listAudioDevices(), api.getAudioStatus()])
      .then(([devices, status]) => {
        this.devices = devices;
        this.status = status;
        this.error = status.lastError;
      })
      .catch((error) => {
        this.error = api.asAppError(error).message;
      })
      .finally(() => {
        this.loading = false;
        this.#request = null;
      });
    return this.#request;
  }

  subscribe() {
    this.#subscribers += 1;
    void this.refresh();
    if (this.#timer === null) {
      this.#timer = window.setInterval(() => void this.refresh(), 5_000);
    }
    return () => {
      this.#subscribers = Math.max(0, this.#subscribers - 1);
      if (this.#subscribers === 0 && this.#timer !== null) {
        window.clearInterval(this.#timer);
        this.#timer = null;
      }
    };
  }
}

export const audioDevices = new AudioDevicesStore();
