- channels are consumers of signal payload
- Each channel gets a channel key as well as user_key and it is upto the user to decide if the user can have multiple channels
- When a signal is emitted, it automatically gets sent to all channel subscribers of that type-id
- Filtering of messages based on predicate is done at the time of subscription .e.g channel.deliver and channel.deliver_if
- There should be provision to store messages externally using redis or something else.
- For each user_id, there should be a message queue implemented as a circular key and each channel holds a cursor to this.
- It should be possible to close/find channel
- for subscription, we should not sue any axum import. there should be utilities or types from vyuh::routes
  - e.g. async fn ws_subscribe(user: authUser, subscription: Subscriber<WS|Poll|SSE>)

# Samples

    '''rust

        async fn subscribe( user: AuthUser, sub: Subscriber<Ws>, channels: Channels) -> Result<Channel> {
            let channel = Channel::new(ChannelKey::new("notifications", user.id)).user(UserKey::new(user.id))
            .deliver::<TaskUpdated>()
            .deliver_if::<NotificationCreated>(move |msg| msg.user_id == user.id);
            sub.attach(channel).await
        }

'''

- These guidelines are guides and not rigid goals

# Cache system

- Entities Cache, Caches, Cached, Cacheable
- System of typed cache with typebased api.
- Cache<T> is object storage for a single type. Caches provides api for all cache
- Cached can be used as return type wrapper around data. returning Cached means that application will 
- Cacheable is a trait that cache objects need to implement optionally. Cached needs this.
- Both Cache, Caches can act as extractor.
- Anything HasSite should provide these.
- Allow pluggable backend defaulting to memory
