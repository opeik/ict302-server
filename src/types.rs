pub mod types {
    use serde::{Deserialize, Serialize};

    type Vec3 = [f32; 3];
    type Id = usize;

    #[derive(Debug)]
    pub enum Permission {
        Login,
        GetEntityData,
    }

    #[derive(Debug)]
    pub struct State {
        pub groups: Vec<Group>,
        pub users: Vec<User>,
        pub entities: Vec<Entity>,
    }

    #[derive(Debug)]
    pub struct Group {
        pub id: Id,
        pub name: String,
        pub permissions: Vec<Permission>,
    }

    #[derive(Debug)]
    pub struct User {
        pub id: Id,
        pub username: String,
        pub password: String,
        pub groups: Vec<Id>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Entity {
        pub id: Id,
        pub position: Vec3,
        pub angles: Vec3,
    }

    impl State {
        pub fn new() -> Self {
            State {
                groups: Vec::new(),
                users: Vec::new(),
                entities: Vec::new(),
            }
        }

        pub fn push_group(&mut self, name: &str, permissions: Vec<Permission>) -> Id {
            let id = self.groups.len();
            self.groups.push(Group::new(id, name, permissions));
            id
        }

        pub fn push_user(&mut self, username: &str, password: &str, groups: Vec<Id>) -> Id {
            let id = self.users.len();
            self.users.push(User::new(id, username, password, groups));
            id
        }

        pub fn push_entity(&mut self, position: Vec3, angles: Vec3) -> Id {
            let id = self.entities.len();
            self.entities.push(Entity::new(id, position, angles));
            id
        }
    }

    impl Group {
        pub fn new(id: Id, name: &str, permissions: Vec<Permission>) -> Self {
            Group {
                id,
                name: String::from(name),
                permissions,
            }
        }
    }

    impl User {
        pub fn new(id: Id, username: &str, password: &str, groups: Vec<Id>) -> Self {
            User {
                id,
                username: String::from(username),
                password: String::from(password),
                groups,
            }
        }
    }

    impl Entity {
        pub fn new(id: Id, position: Vec3, angles: Vec3) -> Self {
            Entity {
                id,
                position,
                angles,
            }
        }
    }
}
