import { test } from "node:test";
import assert from "node:assert/strict";
import { nextSiteKey } from "./tabs.js";

const SAMPLE = [
  { key: "deepseek", title: "DeepSeek" },
  { key: "chatglm", title: "ChatGLM" },
  { key: "zread", title: "Zread" },
  { key: "qianwen", title: "Qianwen" },
];

test("nextSiteKey cycles to the next site in order", () => {
  assert.equal(nextSiteKey(SAMPLE, "deepseek"), "chatglm");
  assert.equal(nextSiteKey(SAMPLE, "chatglm"), "zread");
  assert.equal(nextSiteKey(SAMPLE, "zread"), "qianwen");
});

test("nextSiteKey wraps around from the last site", () => {
  assert.equal(nextSiteKey(SAMPLE, "qianwen"), "deepseek");
});

test("nextSiteKey handles unknown current key by picking first", () => {
  assert.equal(nextSiteKey(SAMPLE, "unknown"), "deepseek");
  assert.equal(nextSiteKey(SAMPLE, ""), "deepseek");
});

test("nextSiteKey returns current key for empty list", () => {
  assert.equal(nextSiteKey([], "deepseek"), "deepseek");
});
