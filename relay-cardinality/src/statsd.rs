use relay_statsd::{CounterMetric, HistogramMetric, TimerMetric};

/// Counter metrics for the Relay Cardinality Limiter.
pub enum CardinalityLimiterCounters {
    /// Incremented for every accepted item by the cardinality limiter.
    ///
    /// This metric is tagged with:
    ///  - `scope`: The scope of check operation.
    #[cfg(feature = "redis")]
    Accepted,
    /// Incremented for every rejected item by the cardinality limiter.
    ///
    /// This metric is tagged with:
    ///  - `scope`: The scope of check operation.
    #[cfg(feature = "redis")]
    Rejected,
    /// Incremented for every hash which was served from the in memory cache.
    ///
    /// This metric is tagged with:
    ///  - `id`: The id of the enforced limit.
    #[cfg(feature = "redis")]
    RedisCacheHit,
    /// Incremented for every hash which was not served from the in memory cache.
    ///
    /// This metric is tagged with:
    ///  - `id`: The id of the enforced limit.
    #[cfg(feature = "redis")]
    RedisCacheMiss,
    /// Amount of entries removed from the cache via periodic cleanups.
    #[cfg(feature = "redis")]
    RedisCacheVacuum,
}

impl CounterMetric for CardinalityLimiterCounters {
    fn name(&self) -> &'static str {
        match *self {
            #[cfg(feature = "redis")]
            Self::Accepted => "cardinality.limiter.accepted",
            #[cfg(feature = "redis")]
            Self::Rejected => "cardinality.limiter.rejected",
            #[cfg(feature = "redis")]
            Self::RedisCacheHit => "cardinality.limiter.redis.cache_hit",
            #[cfg(feature = "redis")]
            Self::RedisCacheMiss => "cardinality.limiter.redis.cache_miss",
            #[cfg(feature = "redis")]
            Self::RedisCacheVacuum => "cardinality.limiter.redis.cache_vacuum",
        }
    }
}

pub enum CardinalityLimiterTimers {
    /// Timer for the entire process of checking cardinality limits.
    CardinalityLimiter,
    /// Timer for the duration of the Redis call.
    ///
    /// This metric is tagged with:
    ///  - `id`: The id of the enforced limit.
    #[cfg(feature = "redis")]
    Redis,
    /// Timer tracking the amount of time spent removing expired values
    /// from the cardinality cache.
    #[cfg(feature = "redis")]
    CacheVacuum,
}

impl TimerMetric for CardinalityLimiterTimers {
    fn name(&self) -> &'static str {
        match self {
            CardinalityLimiterTimers::CardinalityLimiter => "cardinality.limiter.duration",
            #[cfg(feature = "redis")]
            CardinalityLimiterTimers::Redis => "cardinality.limiter.redis.duration",
            #[cfg(feature = "redis")]
            CardinalityLimiterTimers::CacheVacuum => {
                "cardinality.limiter.redis.cache_vacuum.duration"
            }
        }
    }
}

pub enum CardinalityLimiterHistograms {
    /// Amount of hashes sent to Redis to check the cardinality.
    ///
    /// This metric is tagged with:
    ///  - `id`: The id of the enforced limit.
    #[cfg(feature = "redis")]
    RedisCheckHashes,
    /// Redis stored set cardinality.
    ///
    /// This metric is tagged with:
    ///  - `id`: The id of the enforced limit.
    #[cfg(feature = "redis")]
    RedisSetCardinality,
}

impl HistogramMetric for CardinalityLimiterHistograms {
    fn name(&self) -> &'static str {
        match *self {
            #[cfg(feature = "redis")]
            Self::RedisCheckHashes => "cardinality.limiter.redis.check_hashes",
            #[cfg(feature = "redis")]
            Self::RedisSetCardinality => "cardinality.limiter.redis.set_cardinality",
        }
    }
}
