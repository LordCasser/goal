/* eslint-disable @typescript-eslint/prefer-for-of */
/* eslint-disable no-undef */
class DeepgramPcmWorklet extends AudioWorkletProcessor {
  constructor(options) {
    super();

    const processorOptions = options.processorOptions ?? {};
    this.inputSampleRate =
      processorOptions.inputSampleRate || globalThis.sampleRate || 48000;
    this.outputSampleRate = processorOptions.outputSampleRate || 16000;
    this.frameSampleCount = Math.max(
      1,
      Math.round(
        (this.outputSampleRate * (processorOptions.frameMs || 80)) / 1000
      )
    );
    this.inputBuffer = [];
    this.outputBuffer = [];

    this.port.postMessage({
      type: "debug",
      inputSampleRate: this.inputSampleRate,
      outputSampleRate: this.outputSampleRate,
    });
    this.port.onmessage = (event) => {
      if (event.data?.type !== "flush") {
        return;
      }

      this.flushPendingAudio();
      this.port.postMessage({ type: "flushed" });
    };
  }

  process(inputs) {
    const input = inputs[0]?.[0];
    if (!input || input.length === 0) {
      return true;
    }

    for (let index = 0; index < input.length; index += 1) {
      this.inputBuffer.push(input[index]);
    }

    const downsampled = this.downsampleAvailableInput();
    for (let index = 0; index < downsampled.length; index += 1) {
      this.outputBuffer.push(downsampled[index]);
    }

    while (this.outputBuffer.length >= this.frameSampleCount) {
      const frame = this.outputBuffer.slice(0, this.frameSampleCount);
      this.outputBuffer = this.outputBuffer.slice(this.frameSampleCount);
      const buffer = this.floatToLinear16(frame);

      this.postAudioFrame(buffer, frame.length);
    }

    return true;
  }

  flushPendingAudio() {
    const downsampled = this.downsampleAvailableInput();
    for (let index = 0; index < downsampled.length; index += 1) {
      this.outputBuffer.push(downsampled[index]);
    }

    if (this.outputBuffer.length === 0) {
      return;
    }

    const frame = this.outputBuffer;
    this.outputBuffer = [];
    const buffer = this.floatToLinear16(frame);
    this.postAudioFrame(buffer, frame.length);
  }

  postAudioFrame(buffer, sampleCount) {
    this.port.postMessage(
      {
        type: "audio",
        buffer,
        sampleCount,
      },
      [buffer]
    );
  }

  downsampleAvailableInput() {
    if (this.inputSampleRate === this.outputSampleRate) {
      const output = this.inputBuffer;
      this.inputBuffer = [];
      return output;
    }

    const ratio = this.inputSampleRate / this.outputSampleRate;
    const outputSampleCount = Math.floor(this.inputBuffer.length / ratio);
    if (outputSampleCount <= 0) {
      return [];
    }

    const output = new Array(outputSampleCount);
    for (let outputIndex = 0; outputIndex < outputSampleCount; outputIndex += 1) {
      const inputStart = Math.floor(outputIndex * ratio);
      const inputEnd = Math.min(
        this.inputBuffer.length,
        Math.max(inputStart + 1, Math.floor((outputIndex + 1) * ratio))
      );
      let sum = 0;

      for (let inputIndex = inputStart; inputIndex < inputEnd; inputIndex += 1) {
        sum += this.inputBuffer[inputIndex];
      }

      output[outputIndex] = sum / (inputEnd - inputStart);
    }

    const consumedInputSamples = Math.floor(outputSampleCount * ratio);
    this.inputBuffer = this.inputBuffer.slice(consumedInputSamples);
    return output;
  }

  floatToLinear16(samples) {
    const buffer = new ArrayBuffer(samples.length * 2);
    const view = new DataView(buffer);

    for (let index = 0; index < samples.length; index += 1) {
      const sample = Math.max(-1, Math.min(1, samples[index]));
      const value = sample < 0 ? sample * 0x8000 : sample * 0x7fff;
      view.setInt16(index * 2, value, true);
    }

    return buffer;
  }
}

registerProcessor("deepgram-pcm-worklet", DeepgramPcmWorklet);
