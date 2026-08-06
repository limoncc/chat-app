import { test } from "node:test";
import assert from "node:assert/strict";
import { SITES, DEFAULT_TAB, nextActiveKey, WINDOW_ACTIONS } from "./tabs.js";

test("SITES contains three sites in tab order", () => {
  assert.deepEqual(
    SITES.map((s) => s.key),
    ["deepseek", "chatglm", "zread"],
  );
  for (const s of SITES) {
    assert.ok(s.key.length > 0);
    assert.ok(s.title.length > 0);
  }
});

test("DEFAULT_TAB is deepseek", () => {
  assert.equal(DEFAULT_TAB, "deepseek");
});

test("nextActiveKey switches to a known key", () => {
  assert.equal(nextActiveKey(SITES, "deepseek", "chatglm"), "chatglm");
  assert.equal(nextActiveKey(SITES, "chatglm", "zread"), "zread");
});

test("nextActiveKey ignores unknown key and keeps current", () => {
  assert.equal(nextActiveKey(SITES, "deepseek", "unknown"), "deepseek");
  assert.equal(nextActiveKey(SITES, "deepseek", ""), "deepseek");
});

test("nextActiveKey ignores clicking the already-active tab", () => {
  assert.equal(nextActiveKey(SITES, "chatglm", "chatglm"), "chatglm");
});

test("WINDOW_ACTIONS contains the three supported window controls", () => {
  assert.deepEqual(WINDOW_ACTIONS, ["minimize", "toggle_maximize", "close"]);
});
