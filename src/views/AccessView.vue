<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import {
  api,
  type AccessDomain,
  type AccessRole,
  type AccessUser,
  type AclEntry,
} from "../api";
import { activeId } from "../stores/connections";
import { toast } from "../stores/toast";

const users = ref<AccessUser[]>([]);
const domains = ref<AccessDomain[]>([]);
const roles = ref<AccessRole[]>([]);
const acl = ref<AclEntry[]>([]);
const loading = ref(false);
const error = ref("");
const confirmDeleteUser = ref("");
const confirmRevoke = ref("");

// Add-user form.
const newUserName = ref("");
const newUserRealm = ref("pve");
const newUserPassword = ref("");
const newUserComment = ref("");

// Grant-ACL form.
const aclPath = ref("/");
const aclUser = ref("");
const aclRole = ref("PVEAuditor");

function aclKey(e: AclEntry): string {
  return `${e.path}|${e.ugid}|${e.roleid}`;
}

async function refresh() {
  if (!activeId.value) {
    users.value = [];
    acl.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  confirmDeleteUser.value = "";
  confirmRevoke.value = "";
  try {
    [users.value, domains.value, roles.value, acl.value] = await Promise.all([
      api.accessUsers(activeId.value),
      api.accessDomains(activeId.value),
      api.accessRoles(activeId.value),
      api.accessAcl(activeId.value),
    ]);
    users.value.sort((a, b) => a.userid.localeCompare(b.userid));
    roles.value.sort((a, b) => a.roleid.localeCompare(b.roleid));
    acl.value.sort((a, b) => a.path.localeCompare(b.path));
    if (!aclUser.value && users.value.length > 0) aclUser.value = users.value[0].userid;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function addUser() {
  if (!activeId.value || !newUserName.value) return;
  try {
    const params: Record<string, string> = {
      userid: `${newUserName.value}@${newUserRealm.value}`,
    };
    if (newUserPassword.value) params.password = newUserPassword.value;
    if (newUserComment.value) params.comment = newUserComment.value;
    await api.addUser(activeId.value, params);
    toast(`User ${params.userid} created`);
    newUserName.value = newUserPassword.value = newUserComment.value = "";
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function removeUser(userid: string) {
  if (confirmDeleteUser.value !== userid) {
    confirmDeleteUser.value = userid;
    return;
  }
  confirmDeleteUser.value = "";
  if (!activeId.value) return;
  try {
    await api.deleteUser(activeId.value, userid);
    toast(`User ${userid} deleted`);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function grantAcl() {
  if (!activeId.value || !aclPath.value || !aclUser.value || !aclRole.value) return;
  try {
    await api.setAcl(activeId.value, {
      path: aclPath.value,
      users: aclUser.value,
      roles: aclRole.value,
    });
    toast(`Granted ${aclRole.value} on ${aclPath.value}`);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function revokeAcl(e: AclEntry) {
  const key = aclKey(e);
  if (confirmRevoke.value !== key) {
    confirmRevoke.value = key;
    return;
  }
  confirmRevoke.value = "";
  if (!activeId.value) return;
  try {
    const params: Record<string, string> = {
      path: e.path,
      roles: e.roleid,
      delete: "1",
    };
    // The grantee param name depends on the entry type.
    if (e.type === "group") params.groups = e.ugid;
    else if (e.type === "token") params.tokens = e.ugid;
    else params.users = e.ugid;
    await api.setAcl(activeId.value, params);
    toast(`Revoked ${e.roleid} on ${e.path}`);
    await refresh();
  } catch (err) {
    toast(String(err), "error");
  }
}

onMounted(refresh);
watch(activeId, refresh);
</script>

<template>
  <div>
    <div class="head">
      <h1>Access control</h1>
      <button @click="refresh">
        Refresh
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>
    <p
      v-else-if="error"
      class="error"
    >
      {{ error }}
    </p>
    <p v-else-if="loading">
      Loading…
    </p>

    <template v-else-if="activeId">
      <h2>Users</h2>
      <div class="card row">
        <label>
          Username
          <input
            v-model="newUserName"
            placeholder="alice"
          >
        </label>
        <label>
          Realm
          <select v-model="newUserRealm">
            <option
              v-for="d in domains"
              :key="d.realm"
              :value="d.realm"
            >{{ d.realm }}</option>
          </select>
        </label>
        <label>
          Password
          <input
            v-model="newUserPassword"
            type="password"
            autocomplete="new-password"
            placeholder="pve realm only"
          >
        </label>
        <label>
          Comment
          <input v-model="newUserComment">
        </label>
        <button
          :disabled="!newUserName"
          @click="addUser"
        >
          Add user
        </button>
      </div>

      <table v-if="users.length > 0">
        <thead>
          <tr>
            <th>User</th>
            <th>Enabled</th>
            <th>Comment</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="u in users"
            :key="u.userid"
          >
            <td>{{ u.userid }}</td>
            <td>
              <span :class="u.enable !== 0 ? 'ok' : 'off'">
                {{ u.enable !== 0 ? "yes" : "no" }}
              </span>
            </td>
            <td>{{ u.comment ?? "—" }}</td>
            <td>
              <button
                class="danger"
                @click="removeUser(u.userid)"
              >
                {{ confirmDeleteUser === u.userid ? "Confirm?" : "Delete" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>

      <h2>Permissions (ACL)</h2>
      <div class="card row">
        <label>
          Path
          <input
            v-model="aclPath"
            placeholder="/ or /vms/100"
          >
        </label>
        <label>
          User
          <select v-model="aclUser">
            <option
              v-for="u in users"
              :key="u.userid"
              :value="u.userid"
            >{{ u.userid }}</option>
          </select>
        </label>
        <label>
          Role
          <select v-model="aclRole">
            <option
              v-for="r in roles"
              :key="r.roleid"
              :value="r.roleid"
            >{{ r.roleid }}</option>
          </select>
        </label>
        <button
          :disabled="!aclPath || !aclUser"
          @click="grantAcl"
        >
          Grant
        </button>
      </div>

      <table v-if="acl.length > 0">
        <thead>
          <tr>
            <th>Path</th>
            <th>Grantee</th>
            <th>Type</th>
            <th>Role</th>
            <th>Propagate</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="e in acl"
            :key="aclKey(e)"
          >
            <td>{{ e.path }}</td>
            <td>{{ e.ugid }}</td>
            <td>{{ e.type }}</td>
            <td>{{ e.roleid }}</td>
            <td>{{ e.propagate ? "yes" : "no" }}</td>
            <td>
              <button
                class="danger"
                @click="revokeAcl(e)"
              >
                {{ confirmRevoke === aclKey(e) ? "Confirm?" : "Revoke" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No ACL entries.
      </p>

      <h2>Realms</h2>
      <table v-if="domains.length > 0">
        <thead>
          <tr>
            <th>Realm</th>
            <th>Type</th>
            <th>Default</th>
            <th>Comment</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="d in domains"
            :key="d.realm"
          >
            <td>{{ d.realm }}</td>
            <td>{{ d.type ?? "—" }}</td>
            <td>{{ d.default ? "yes" : "no" }}</td>
            <td>{{ d.comment ?? "—" }}</td>
          </tr>
        </tbody>
      </table>
    </template>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: 16px;
}

.head h1 {
  margin-right: auto;
}

h2 {
  margin-top: 28px;
}

.card {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 12px 16px;
  margin: 12px 0;
}

.row {
  display: flex;
  gap: 12px;
  align-items: flex-end;
  flex-wrap: wrap;
}

label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 0.9em;
}

.row input {
  width: 140px;
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  text-align: left;
  padding: 6px 10px;
  border-bottom: 1px solid #ccc3;
}

.ok {
  color: #2a7;
}

.off {
  opacity: 0.6;
}

.danger {
  color: #c33;
}

.error {
  color: #c33;
}
</style>
