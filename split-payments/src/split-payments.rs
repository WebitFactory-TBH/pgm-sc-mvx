#![no_std]
#![no_main]
#![allow(unused_attributes)]

multiversx_sc::imports!();
multiversx_sc::derive_imports!();

pub const COMMISSION_RATE_PERCENT: u64 = 1;

#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, TypeAbi, Clone, ManagedVecItem)]
pub struct IndividualPayment<M: ManagedTypeApi> {
    pub amount: BigUint<M>,
    pub destination: ManagedAddress<M>,
}

#[derive(
    TopEncode, TopDecode, NestedEncode, NestedDecode, TypeAbi, PartialEq, Clone, ManagedVecItem,
)]
pub enum Status {
    Pending,
    Completed,
    Cancelled,
}

#[derive(TopEncode, TopDecode, NestedEncode, NestedDecode, TypeAbi, Clone)]
pub struct PaymentLink<M: ManagedTypeApi> {
    pub payment_id: String,
    pub payments: ManagedVec<M, IndividualPayment<M>>,
    pub status: Status,
    pub creator: ManagedAddress<M>,
}

#[multiversx_sc::contract]
pub trait SplitPaymentsContract {
    // Storage
    #[view(isEnabled)]
    #[storage_mapper("enabled")]
    fn enabled(&self) -> SingleValueMapper<Self::Api, bool>;

    #[view(getOwner)]
    #[storage_mapper("owner")]
    fn owner(&self) -> SingleValueMapper<Self::Api, ManagedAddress<Self::Api>>;

    #[view(getCommissionRate)]
    #[storage_mapper("commission_rate")]
    fn commission_rate(&self) -> SingleValueMapper<Self::Api, BigUint>;

    #[storage_mapper("payment_links")]
    fn payment_links(&self) -> MapMapper<Self::Api, String, PaymentLink<Self::Api>>;

    // Events
    #[event("createdPaymentLink")]
    fn created_payment_link_event(
        &self,
        #[indexed] payment_id: String,
        #[indexed] creator: ManagedAddress<Self::Api>,
    );

    #[event("completedPayment")]
    fn completed_payment_event(&self, #[indexed] payment_id: String);

    #[event("cancelledPayment")]
    fn cancelled_payment_event(&self, #[indexed] payment_id: String);

    #[event("individualPaymentCompleted")]
    fn individual_payment_completed_event(
        &self,
        #[indexed] payment_id: String,
        #[indexed] from: ManagedAddress<Self::Api>,
        #[indexed] destination: ManagedAddress<Self::Api>,
        amount: BigUint,
    );

    // Guards
    fn guard_payment_link_exists(&self, payment_id: String) {
        require!(
            self.payment_links().contains_key(&payment_id),
            "Payment link does not exist"
        );
    }

    fn guard_payment_has_status(&self, payment_id: String, status: Status) {
        let payment_link = self.payment_links().get(&payment_id);

        require!(
            payment_link.unwrap().status == status,
            "Payment link does not have the required status"
        );
    }

    fn guard_sent_enough_egld(&self, payment_id: String) {
        let payment_link = self.payment_links().get(&payment_id);

        let total_amount = self._compute_total_amount(&payment_link.unwrap().payments);
        let call_value = self.call_value().egld_value();

        require!(
            call_value >= total_amount,
            "Not enough EGLD sent to complete payment (check required/minimum amount)"
        );
    }

    fn guard_only_owner(&self) {
        let caller = self.blockchain().get_caller();
        let owner = self.owner().get();

        require!(caller == owner, "Only owner can call this function");
    }

    fn guard_only_payment_creator(&self, payment_id: String) {
        let caller = self.blockchain().get_caller();
        let payment_link = self.payment_links().get(&payment_id).unwrap();

        require!(
            caller == payment_link.creator,
            "Only payment link creator can call this function"
        );
    }

    fn guard_if_enabled(&self) {
        require!(self.enabled().get(), "Contract is disabled");
    }

    // Endpoints
    #[init]
    fn init(&self) {
        self.commission_rate()
            .set(BigUint::from(COMMISSION_RATE_PERCENT));
        self.owner().set(&self.blockchain().get_caller());
        self.enabled().set(&true);
    }

    #[endpoint(createPaymentLink)]
    fn create_payment_link(
        &self,
        payment_id: String,
        payments: &ManagedVec<Self::Api, IndividualPayment<Self::Api>>,
    ) {
        self.guard_if_enabled();

        let payment_link = PaymentLink {
            payment_id: payment_id.clone(),
            status: Status::Pending,
            payments: payments.clone(),
            creator: self.blockchain().get_caller(),
        };

        self.payment_links()
            .insert(payment_id.clone(), payment_link.clone());

        self.created_payment_link_event(payment_id.clone(), payment_link.creator);
    }

    #[payable("EGLD")]
    #[endpoint(completePayment)]
    fn complete_payment(&self, payment_id: String) {
        self.guard_if_enabled();
        self.guard_payment_link_exists(payment_id.clone());
        self.guard_payment_has_status(payment_id.clone(), Status::Pending);
        self.guard_sent_enough_egld(payment_id.clone());

        let mut used_amount = BigUint::zero();
        let mut payment_link = self.payment_links().get(&payment_id).unwrap();

        // Transfer funds
        for payment in payment_link.payments.iter() {
            self.send()
                .direct_egld(&payment.destination, &payment.amount);
            used_amount += &payment.amount;

            self.individual_payment_completed_event(
                payment_id.clone(),
                self.blockchain().get_caller(),
                payment.destination.clone(),
                payment.amount.clone(),
            );
        }

        let commission_amount =
            &used_amount * &self.commission_rate().get() / BigUint::from(100u64);

        // Retrieve extra funds
        let call_value = self.call_value().egld_value();
        let extra_amount = &call_value - &used_amount - &commission_amount;
        self.send()
            .direct_egld(&self.blockchain().get_caller(), &extra_amount);

        // Set payment link status to completed
        payment_link.status = Status::Completed;
        self.payment_links()
            .insert(payment_id.clone(), payment_link.clone());
        self.completed_payment_event(payment_id.clone());
    }

    #[endpoint(cancelPayment)]
    fn cancel_payment(&self, payment_id: String) {
        self.guard_if_enabled();
        self.guard_only_payment_creator(payment_id.clone());
        self.guard_payment_link_exists(payment_id.clone());
        self.guard_payment_has_status(payment_id.clone(), Status::Pending);

        let mut payment_link = self.payment_links().get(&payment_id).unwrap();
        payment_link.status = Status::Cancelled;
        self.payment_links()
            .insert(payment_id.clone(), payment_link.clone());

        self.cancelled_payment_event(payment_id.clone());
    }

    #[endpoint(setCommissionRate)]
    fn set_commission_rate(&self, commission_rate: BigUint<Self::Api>) {
        self.guard_if_enabled();
        self.guard_only_owner();

        require!(
            commission_rate <= BigUint::from(100u64),
            "Commission rate cannot be greater than 100%"
        );

        self.commission_rate().set(&commission_rate);
    }

    #[endpoint(disableContract)]
    fn disable_contract(&self) {
        self.guard_if_enabled();
        self.guard_only_owner();

        self.enabled().set(&false);
    }

    #[endpoint(enableContract)]
    fn enable_contract(&self) {
        require!(!self.enabled().get(), "Contract is already enabled");
        self.guard_only_owner();

        self.enabled().set(&true);
    }

    // Views
    #[view(getRequiredAmount)]
    fn get_required_amount(&self, payment_id: String) -> BigUint<Self::Api> {
        self.guard_payment_link_exists(payment_id.clone());
        self.guard_payment_has_status(payment_id.clone(), Status::Pending);

        let payment_link = self.payment_links().get(&payment_id).unwrap();
        self._compute_total_amount(&payment_link.payments)
    }

    #[view(getPaymentStatus)]
    fn get_payment_status(&self, payment_id: String) -> Status {
        self.guard_payment_link_exists(payment_id.clone());

        let payment_link = self.payment_links().get(&payment_id).unwrap();
        payment_link.status
    }

    // Utils
    fn _compute_total_amount(
        &self,
        payments: &ManagedVec<Self::Api, IndividualPayment<Self::Api>>,
    ) -> BigUint<Self::Api> {
        let mut total_amount = BigUint::zero();
        for payment in payments.iter() {
            total_amount += &payment.amount;
        }

        // Add commission
        let commission_amount = &total_amount * COMMISSION_RATE_PERCENT / 100u64;
        total_amount += &commission_amount;

        total_amount
    }
}
