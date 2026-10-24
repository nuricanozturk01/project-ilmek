"use strict";
(() => {
  // src/types.ts
  var IlmekError = class extends Error {
    code;
    retryable;
    /** The request may or may not have been applied; retry with the same operation id. */
    outcomeUnknown;
    operationId;
    constructor(failure) {
      super(failure.message);
      this.name = "IlmekError";
      this.code = failure.code;
      this.retryable = failure.retryable;
      this.outcomeUnknown = failure.outcomeUnknown;
      this.operationId = failure.operationId;
    }
  };

  // src/uuid7.ts
  var secureRandom = (length) => {
    const bytes = new Uint8Array(length);
    globalThis.crypto.getRandomValues(bytes);
    return bytes;
  };
  function uuidv7(now = Date.now(), random = secureRandom) {
    const bytes = new Uint8Array(16);
    let remaining = Math.floor(now);
    for (let index = 5; index >= 0; index -= 1) {
      bytes[index] = remaining % 256;
      remaining = Math.floor(remaining / 256);
    }
    bytes.set(random(10), 6);
    bytes[6] = (bytes[6] ?? 0) & 15 | 112;
    bytes[8] = (bytes[8] ?? 0) & 63 | 128;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }
  var UUID7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
  function isUuidv7(text) {
    return UUID7.test(text);
  }

  // src/sdk.ts
  var METHOD = /^[a-z][a-z0-9_.]{0,63}$/;
  function fail(code, message) {
    return new IlmekError({ code, message, retryable: false, outcomeUnknown: false });
  }
  function isRecord(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }
  function readFailure(value) {
    if (isRecord(value) && typeof value.code === "string" && typeof value.message === "string") {
      return {
        code: value.code,
        message: value.message,
        retryable: value.retryable === true,
        outcomeUnknown: value.outcomeUnknown === true,
        ...typeof value.operationId === "string" ? { operationId: value.operationId } : {}
      };
    }
    return {
      code: "INTERNAL",
      message: "malformed error from the runtime",
      retryable: false,
      outcomeUnknown: false
    };
  }
  function createSdk(runtime, app) {
    const subscriptions = /* @__PURE__ */ new Map();
    const connectionHandlers = /* @__PURE__ */ new Set();
    const newOperationId = runtime.newOperationId ?? uuidv7;
    async function request(message) {
      const response = await runtime.transport.request(message);
      if (!isRecord(response) || typeof response.ok !== "boolean") {
        throw fail("INTERNAL", "malformed response from the runtime");
      }
      if (!response.ok) throw new IlmekError(readFailure(response.error));
      return response.result;
    }
    runtime.setEventSink((event) => {
      if (event.kind === "connection") {
        for (const handler of connectionHandlers) handler(event.state);
      } else if (event.kind === "event") {
        subscriptions.get(event.subscriptionId)?.(event.event);
      } else {
        subscriptions.delete(event.subscriptionId);
      }
    });
    async function call(method, input, options = {}) {
      if (!METHOD.test(method)) throw fail("INVALID_MESSAGE", "method name is not valid");
      if (!isRecord(input)) throw fail("INVALID_MESSAGE", "input must be a JSON object");
      const write = options.write ?? options.operationId !== void 0;
      if (options.operationId !== void 0 && !isUuidv7(options.operationId)) {
        throw fail("INVALID_MESSAGE", "operationId must be a UUIDv7");
      }
      const operationId = write ? options.operationId ?? newOperationId() : null;
      const result = await request({
        v: 1,
        op: "call",
        method,
        input,
        write,
        operationId,
        timeoutMs: options.timeoutMs ?? null
      });
      return result;
    }
    return {
      app: { ...app },
      connection: {
        async state() {
          const state = await request({ v: 1, op: "connection.state" });
          return state === "connected" ? "connected" : "disconnected";
        },
        onChange(handler) {
          connectionHandlers.add(handler);
          return () => connectionHandlers.delete(handler);
        }
      },
      call,
      async subscribe(topic, cursor, handler) {
        if (!METHOD.test(topic)) throw fail("INVALID_MESSAGE", "topic name is not valid");
        const result = await request({ v: 1, op: "subscribe", topic, cursor });
        const id = isRecord(result) ? result.subscriptionId : void 0;
        if (typeof id !== "string") throw fail("INTERNAL", "malformed subscription from the runtime");
        subscriptions.set(id, handler);
        return () => {
          subscriptions.delete(id);
          void request({ v: 1, op: "unsubscribe", subscriptionId: id }).catch(() => void 0);
        };
      },
      storage: {
        get: (key) => request({ v: 1, op: "storage.get", key }),
        async set(key, value) {
          await request({ v: 1, op: "storage.set", key, value });
        },
        async remove(key) {
          await request({ v: 1, op: "storage.remove", key });
        }
      }
    };
  }

  // src/inject.ts
  function isMessage(data) {
    return typeof data === "object" && data !== null && data.ilmek === 1;
  }
  function install(target) {
    const bootstrap = target.__ILMEK_BOOTSTRAP__;
    const shell = target.parent;
    if (bootstrap === void 0 || shell === void 0 || shell === target) return;
    delete target.__ILMEK_BOOTSTRAP__;
    const pending = /* @__PURE__ */ new Map();
    let counter = 0;
    let sink = () => void 0;
    target.addEventListener("message", (event) => {
      if (event.source !== shell || !isMessage(event.data)) return;
      const message = event.data;
      if (typeof message.event === "string") {
        sink(JSON.parse(message.event));
        return;
      }
      if (typeof message.id !== "string") return;
      const waiting = pending.get(message.id);
      if (waiting === void 0) return;
      pending.delete(message.id);
      waiting.resolve(message.response);
    });
    const runtime = {
      transport: {
        request: (request) => new Promise((resolve) => {
          counter += 1;
          const id = `r${String(counter)}`;
          pending.set(id, { resolve });
          shell.postMessage({ ilmek: 1, id, request }, "*");
        })
      },
      setEventSink: (next) => {
        sink = next;
      }
    };
    Object.defineProperty(target, "ilmek", {
      value: Object.freeze(createSdk(runtime, bootstrap)),
      writable: false,
      configurable: false
    });
  }
  install(globalThis);
})();
