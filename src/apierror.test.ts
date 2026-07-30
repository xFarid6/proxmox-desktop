import { describe, expect, it } from "vitest";
import { explainError, missingPrivilege } from "./apierror";

// The 403 string is the one a real cluster produced on the Firewall and HA
// tabs (#90) — token without Sys.Audit, copied verbatim from the report.
const PERM_403 =
  'Proxmox API error (403): {"message":"Permission check failed (/, Sys.Audit)\\n","data":null}';

describe("missingPrivilege", () => {
  it("pulls the path and privilege out of a permission failure", () => {
    expect(missingPrivilege(PERM_403)).toEqual({ path: "/", privilege: "Sys.Audit" });
  });

  it("handles a guest-scoped path", () => {
    expect(
      missingPrivilege(
        'Proxmox API error (403): {"message":"Permission check failed (/vms/100, VM.Backup)\\n"}',
      ),
    ).toEqual({ path: "/vms/100", privilege: "VM.Backup" });
  });

  it("is null for anything that is not a 403 permission failure", () => {
    expect(missingPrivilege("Proxmox API error (500): rados_connect failed")).toBeNull();
    expect(missingPrivilege('Proxmox API error (403): {"message":"forbidden"}')).toBeNull();
    expect(missingPrivilege("HTTP error: connection refused")).toBeNull();
  });
});

describe("explainError", () => {
  it("names the missing privilege and drops the JSON blob", () => {
    const msg = explainError(PERM_403);
    expect(msg).toContain("Sys.Audit");
    expect(msg).toContain("the whole datacenter");
    expect(msg).not.toContain("{");
  });

  it("explains a 401 as a bad token rather than a bad host", () => {
    expect(explainError('Proxmox API error (401): {"data":null}')).toContain("Token ID");
  });

  it("still explains a 403 whose body has no permission wording", () => {
    expect(explainError('Proxmox API error (403): {"message":"forbidden"}')).toContain("403");
  });

  it("passes anything else through unchanged", () => {
    const other = "HTTP error: error sending request for url (https://pve:8006/api2/json/version)";
    expect(explainError(other)).toBe(other);
    expect(explainError("Proxmox API error (500): rados_connect failed")).toContain(
      "rados_connect failed",
    );
  });
});
