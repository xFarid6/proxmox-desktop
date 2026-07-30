import { describe, expect, it } from "vitest";
import { joinToken, tokenProblem } from "./token";

describe("joinToken", () => {
  it("joins the two halves with the separator PVE wants", () => {
    expect(joinToken("root@pam!desktop", "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")).toBe(
      "root@pam!desktop=aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
    );
  });

  it("trims what was pasted", () => {
    expect(joinToken("  root@pam!desktop ", " uuid\n")).toBe("root@pam!desktop=uuid");
  });

  it("takes an already-complete token pasted into the first field", () => {
    expect(joinToken("root@pam!desktop=uuid", "")).toBe("root@pam!desktop=uuid");
  });

  it("is empty when either half is missing, so nothing half-formed is sent", () => {
    expect(joinToken("", "")).toBe("");
    expect(joinToken("root@pam!desktop", "")).toBe("");
    expect(joinToken("", "uuid")).toBe("");
  });
});

describe("tokenProblem", () => {
  it("accepts a well-formed pair", () => {
    expect(tokenProblem("root@pam!desktop", "uuid")).toBe("");
  });

  it("treats both-empty as fine — it means unchanged", () => {
    expect(tokenProblem("", "")).toBe("");
  });

  it("names the missing half", () => {
    expect(tokenProblem("root@pam!desktop", "")).toContain("secret");
    expect(tokenProblem("", "uuid")).toContain("Token ID");
  });

  // The exact mistake #86 is about: the token dialog's two values joined by
  // hand with the wrong separator, which PVE answers with a bare 401.
  it("rejects a Token ID that is really a mis-joined pair", () => {
    expect(tokenProblem("root@pam!desktop:uuid", "uuid")).toContain("not a Token ID");
  });

  it("rejects an id missing its realm or its token name", () => {
    expect(tokenProblem("root!desktop", "uuid")).toContain("not a Token ID");
    expect(tokenProblem("root@pam", "uuid")).toContain("not a Token ID");
  });

  it("says nothing about a complete token pasted into the first field", () => {
    expect(tokenProblem("root@pam!desktop=uuid", "")).toBe("");
  });
});
