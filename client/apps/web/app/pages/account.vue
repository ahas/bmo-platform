<!-- The account's settings stand in the settings dialog over whatever screen is open (docs/web.md,
     「Settings」); this address takes a reader home with the dialog open at the account section. -->
<script setup lang="ts">
import { MEMBER_HOME, isAdministrator } from "~/domain/access";
import { SETTINGS_QUERY } from "~/domain/settings-dialog";

definePageMeta({
  member: true,
  middleware: () => {
    const { session } = useUserSession();

    return navigateTo({ path: isAdministrator(session.value) ? "/" : MEMBER_HOME, query: { [SETTINGS_QUERY]: "account" } }, { replace: true });
  }
});
</script>
