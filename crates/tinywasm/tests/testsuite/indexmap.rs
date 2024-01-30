/// A naive implementation of an index map for use in the test suite
pub struct IndexMap<K, V> {
    map: std::collections::HashMap<K, V>,
    keys: Vec<K>,
}

impl<K, V> IndexMap<K, V>
where
    K: std::cmp::Eq + std::hash::Hash + Clone,
{
    pub fn new() -> Self {
        Self { map: std::collections::HashMap::new(), keys: Vec::new() }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.map.entry(key.clone()) {
            return Some(e.insert(value));
        }

        self.keys.push(key.clone());
        self.map.insert(key, value)
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.keys.iter().map(move |k| (k, self.map.get(k).unwrap()))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.keys.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.map.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.map.values_mut()
    }
}
