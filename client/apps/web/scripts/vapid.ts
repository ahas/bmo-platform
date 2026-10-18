import webpush from "web-push";

// Makes the pair of keys this console signs its push notifications with. They are the server's
// own identity to the push services: the public half goes out to every browser that subscribes
// and the private half signs, so it stays on this machine and never in the repository.
//
// It prints them and writes nothing. Put the two lines it prints into client/.env and restart the
// console. Making a new pair invalidates every subscription made against the old one, so the
// devices already subscribed have to subscribe again.
//
// usage: bun scripts/vapid.ts

const keys = webpush.generateVAPIDKeys();

console.log(`BMO_VAPID_PUBLIC_KEY=${keys.publicKey}`);
console.log(`BMO_VAPID_PRIVATE_KEY=${keys.privateKey}`);
