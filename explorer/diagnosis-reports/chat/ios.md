## Truapi iOS Chat Diagnosis

**5 success · 0 failed**

| Method | Status | Details |
| --- | --- | --- |
| `Chat/create_room` | ✅ | created once, then returned Exists |
| `Chat/list_subscribe` | ✅ | observed the newly created room |
| `Chat/post_message` | ✅ | posted text and custom messages |
| `Chat/action_subscribe` | ✅ | received MessagePosted with the originating room |
| `Chat/custom_message_render` | ✅ | served initial and replacement trees on a host-initiated render stream |
