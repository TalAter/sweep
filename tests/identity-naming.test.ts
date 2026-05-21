import { describe, expect, test } from "bun:test";
import { slugFromUrl } from "../src/identity/naming.ts";

describe("slugFromUrl", () => {
  describe("spec examples", () => {
    test("ollama.com/install.sh -> ollama", () => {
      expect(slugFromUrl("https://ollama.com/install.sh")).toBe("ollama");
    });

    test("get.bun.sh -> bun (strips get.)", () => {
      expect(slugFromUrl("https://get.bun.sh")).toBe("bun");
    });

    test("mise.run -> mise", () => {
      expect(slugFromUrl("https://mise.run")).toBe("mise");
    });

    test("install.foo.bar.com -> foo (strips install.)", () => {
      expect(slugFromUrl("https://install.foo.bar.com")).toBe("foo");
    });
  });

  describe("prefix stripping", () => {
    test("strips www.", () => {
      expect(slugFromUrl("https://www.example.com")).toBe("example");
    });

    test("strips get.", () => {
      expect(slugFromUrl("https://get.example.com")).toBe("example");
    });

    test("strips install.", () => {
      expect(slugFromUrl("https://install.example.com")).toBe("example");
    });

    test("strips download.", () => {
      expect(slugFromUrl("https://download.example.com")).toBe("example");
    });

    test("strips dl.", () => {
      expect(slugFromUrl("https://dl.example.com")).toBe("example");
    });

    test("strips cdn.", () => {
      expect(slugFromUrl("https://cdn.example.com")).toBe("example");
    });

    test("at most ONE strip — www.get.bun.sh strips only www.", () => {
      expect(slugFromUrl("https://www.get.bun.sh")).toBe("get");
    });

    test("prefix label not a separator is NOT stripped (www-bun.sh)", () => {
      expect(slugFromUrl("https://www-bun.sh")).toBe("www-bun");
    });

    test("no prefix match, returns first label", () => {
      expect(slugFromUrl("https://foo.example.com")).toBe("foo");
    });
  });

  describe("case insensitivity", () => {
    test("uppercase hostname is lowercased before prefix match", () => {
      expect(slugFromUrl("https://Get.Bun.SH")).toBe("bun");
    });

    test("mixed-case label is lowercased in result", () => {
      expect(slugFromUrl("https://OLLAMA.com/install.sh")).toBe("ollama");
    });
  });

  describe("invalid URLs return 'unknown'", () => {
    test("non-URL string returns unknown", () => {
      expect(slugFromUrl("not a url")).toBe("unknown");
    });

    test("empty string returns unknown", () => {
      expect(slugFromUrl("")).toBe("unknown");
    });
  });

  describe("edge cases", () => {
    test("IP host returns the first numeric label", () => {
      expect(slugFromUrl("http://192.168.1.1/foo")).toBe("192");
    });

    test("empty hostname (file:///foo) falls back to 'unknown'", () => {
      // file:///foo parses but hostname is "". Empty slug would break list
      // rendering, so we pin this to "unknown".
      expect(slugFromUrl("file:///foo")).toBe("unknown");
    });
  });
});
